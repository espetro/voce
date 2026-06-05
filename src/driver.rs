//! VoceAudio HAL driver installation and shared-memory ring buffer.
//!
//! On first launch, `ensure_installed()` copies `VoceAudio.driver` from the
//! app bundle into `~/Library/Audio/Plug-Ins/HAL/` and signals coreaudiod to
//! reload. Subsequent launches skip the copy if the bundle is already there.
//!
//! `RingBufferWriter` maps the same POSIX shared-memory segment that the HAL
//! plugin reads from, allowing the Rust audio pipeline to push filtered
//! float32 samples directly to the virtual microphone.

use anyhow::{Context, Result, bail};
use std::{
    ffi::CString,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use tracing::{info, warn};

// ── Shared-memory layout (must match VoceAudio.c) ─────────────────────────────

const SHM_NAME: &str = "/voce_audio_ring";
const RING_CAP: usize = 22050 * 4; // 4 seconds of f32

/// Layout mirrors the `VoceRingBuffer` struct in VoceAudio.c.
/// Mapped into both Rust (writer) and the HAL plugin (reader).
#[repr(C)]
struct RingLayout {
    write_pos: AtomicU32,
    read_pos: AtomicU32,
    samples: [f32; RING_CAP],
}

// ── Public ring-buffer writer ──────────────────────────────────────────────────

/// Writes float32 samples to the shared-memory ring buffer consumed by the
/// VoceAudio HAL plugin.  Lock-free producer; safe to call from the audio thread.
pub struct RingBufferWriter {
    ring: *mut RingLayout,
    shm_fd: libc::c_int,
}

// SAFETY: the ring buffer is in shared memory and access is coordinated via
// atomic write_pos/read_pos; we only write from one thread at a time.
unsafe impl Send for RingBufferWriter {}

impl RingBufferWriter {
    /// Open (and create if necessary) the POSIX shared-memory segment.
    pub fn open() -> Result<Self> {
        let name = CString::new(SHM_NAME).unwrap();
        let shm_size = std::mem::size_of::<RingLayout>();

        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR,
                0o600,
            )
        };
        if fd < 0 {
            bail!("shm_open failed: {}", std::io::Error::last_os_error());
        }

        // Size the segment (idempotent if already the right size)
        let rc = unsafe { libc::ftruncate(fd, shm_size as libc::off_t) };
        if rc < 0 {
            unsafe { libc::close(fd) };
            bail!("ftruncate failed: {}", std::io::Error::last_os_error());
        }

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                shm_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            unsafe { libc::close(fd) };
            bail!("mmap failed: {}", std::io::Error::last_os_error());
        }

        info!("Shared memory ring buffer opened ({} bytes)", shm_size);
        Ok(Self {
            ring: ptr as *mut RingLayout,
            shm_fd: fd,
        })
    }

    /// Push a single float32 sample.  Overwrites oldest sample on overflow.
    #[inline]
    pub fn push(&self, sample: f32) {
        // SAFETY: ring is valid mapped memory; atomic ops provide ordering.
        let r = unsafe { &*self.ring };
        let wp = r.write_pos.load(Ordering::Acquire) as usize;
        // SAFETY: wp % RING_CAP is always a valid index.
        unsafe {
            let slot = (self.ring as *mut f32)
                .add(std::mem::offset_of!(RingLayout, samples) / 4 + (wp % RING_CAP));
            slot.write(sample);
        }
        r.write_pos
            .store(((wp + 1) % RING_CAP) as u32, Ordering::Release);
    }

    /// Push a slice of samples.
    pub fn push_slice(&self, samples: &[f32]) {
        for &s in samples {
            self.push(s);
        }
    }
}

impl Drop for RingBufferWriter {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ring as *mut libc::c_void, std::mem::size_of::<RingLayout>());
            libc::close(self.shm_fd);
        }
    }
}

// ── Driver installation ────────────────────────────────────────────────────────

const DRIVER_BUNDLE: &str = "VoceAudio.driver";

/// Returns `~/Library/Audio/Plug-Ins/HAL/VoceAudio.driver`.
fn hal_install_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join("Library/Audio/Plug-Ins/HAL").join(DRIVER_BUNDLE))
}

/// Returns the path to VoceAudio.driver bundled inside the running .app.
/// Falls back to `target/release/VoceAudio.driver` for development builds.
fn bundled_driver_path() -> Option<PathBuf> {
    // Production: <AppBundle>/Contents/PlugIns/VoceAudio.driver
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent()
            .and_then(|p| p.parent()) // .app/Contents/MacOS → .app/Contents
            .map(|p| p.join("PlugIns").join(DRIVER_BUNDLE));
        if let Some(p) = candidate {
            if p.exists() {
                return Some(p);
            }
        }
    }
    // Development: target/release/VoceAudio.driver (built by `make -C audio-driver install`)
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent()
            .map(|p| p.join(DRIVER_BUNDLE));
        if let Some(p) = candidate {
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Returns true if "Voce Microphone" appears in the CoreAudio device list.
pub fn voce_device_found() -> bool {
    // Use system_profiler as a quick check (no-dependency approach)
    Command::new("system_profiler")
        .args(["SPAudioDataType", "-detailLevel", "mini"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Voce Microphone"))
        .unwrap_or(false)
}

/// Install the HAL driver if not already present, then tell coreaudiod to reload.
///
/// Safe to call on every launch — skips the copy if the bundle is already installed.
pub fn ensure_installed() -> Result<()> {
    let install_path = hal_install_path()?;

    if install_path.exists() {
        info!("VoceAudio.driver already installed at {}", install_path.display());
        return Ok(());
    }

    let src = bundled_driver_path()
        .context("VoceAudio.driver not found in app bundle or target/release")?;

    info!("Installing {} → {}", src.display(), install_path.display());

    // Create HAL directory if missing
    if let Some(parent) = install_path.parent() {
        std::fs::create_dir_all(parent)
            .context("cannot create ~/Library/Audio/Plug-Ins/HAL/")?;
    }

    copy_dir_recursive(&src, &install_path)
        .context("failed to copy VoceAudio.driver")?;

    // Signal coreaudiod to reload — non-fatal if it fails
    let status = Command::new("launchctl")
        .args(["kickstart", "-k", "system/com.apple.audio.coreaudiod"])
        .status();
    match status {
        Ok(s) if s.success() => {
            info!("coreaudiod reloading — waiting 1.5s...");
            std::thread::sleep(Duration::from_millis(1500));
        }
        Ok(s) => warn!("launchctl exited {:?}", s.code()),
        Err(e) => warn!("launchctl failed: {e}"),
    }

    info!("VoceAudio.driver installed");
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}
