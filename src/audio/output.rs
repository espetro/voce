use crate::audio::buffer::AudioChunk;
use crate::driver::RingBufferWriter;
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Receiver;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{info, warn};

// Legacy keyword kept for backward-compat detection (used in UI events)
pub const BLACKHOLE_KEYWORD: &str = "blackhole";

/// A live output stream that routes filtered mic audio to the virtual
/// "Voce Microphone" device via shared-memory ring buffer.
///
/// Falls back to the BlackHole device if the ring buffer cannot be opened
/// (e.g., driver not yet installed).
pub struct OutputStream {
    _stream: Option<cpal::Stream>,
    _ring:   Option<Box<RingBufferWriter>>,
}

impl OutputStream {
    /// Start routing: mic → gate → ring buffer → Voce Microphone.
    ///
    /// If opening the ring buffer fails, falls back to BlackHole output so
    /// existing users are not broken during the transition.
    pub fn start(audio_rx: Receiver<AudioChunk>, gate_state: Arc<AtomicBool>) -> Result<Self> {
        // Prefer ring buffer (Voce Microphone HAL driver)
        match RingBufferWriter::open() {
            Ok(ring) => {
                info!("Output: using VoceAudio ring buffer");
                // Box before passing so the heap address is stable — the closure
                // captures a raw pointer to it that must not be invalidated by a move.
                let ring = Box::new(ring);
                let stream = start_ring_stream(audio_rx, gate_state, ring.as_ref())?;
                return Ok(Self {
                    _stream: Some(stream),
                    _ring:   Some(ring),
                });
            }
            Err(e) => warn!("Ring buffer unavailable ({e}); falling back to BlackHole"),
        }

        // Fallback: BlackHole
        let device = find_blackhole_device()
            .context("Neither VoceAudio ring buffer nor BlackHole 2ch found.\n\
                      Run `make -C audio-driver reload` to install the virtual microphone.")?;
        info!("Output fallback device: {}", device.name().unwrap_or_default());
        let stream = start_blackhole_stream(audio_rx, gate_state, &device)?;
        Ok(Self {
            _stream: Some(stream),
            _ring:   None,
        })
    }

    /// Returns the VoceAudio or BlackHole device if present (for status reporting).
    pub fn detect_output_device() -> bool {
        // Ring buffer existence means the HAL driver is installed and coreaudiod loaded it
        crate::driver::voce_device_found() || find_blackhole_device().is_some()
    }

    /// Legacy: returns true if a BlackHole-named output device exists.
    pub fn detect_blackhole() -> bool {
        find_blackhole_device().is_some()
    }
}

// ── Ring-buffer stream ────────────────────────────────────────────────────────

fn start_ring_stream(
    audio_rx: Receiver<AudioChunk>,
    gate_state: Arc<AtomicBool>,
    ring: &RingBufferWriter,
) -> Result<cpal::Stream> {
    // We still need a cpal stream for the mic capture side-effect: the stream
    // keeps capture running, but filtered output is pushed to the ring buffer.
    // Use the default output device just to have a real-time callback that
    // drains the channel; actual audio data goes to the ring, not the cpal buffer.
    let host = cpal::default_host();

    // Try to find a non-BlackHole, non-Voce output device for the dummy stream
    let device = find_non_virtual_output_device(&host)
        .or_else(|| host.default_output_device())
        .context("no output device available")?;

    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: 48000,
        buffer_size: cpal::BufferSize::Default,
    };

    // SAFETY: RingBufferWriter is Send; we move a raw pointer across the closure.
    // The ring lives as long as OutputStream (held in _ring), so the pointer is valid
    // for the lifetime of the stream.
    let ring_ptr = ring as *const RingBufferWriter as usize;

    let mut mic_buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(4096);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                while let Ok(chunk) = audio_rx.try_recv() {
                    mic_buf.extend(chunk.samples.iter().copied());
                }

                let pass = gate_state.load(Ordering::Relaxed);
                // SAFETY: ring_ptr is valid (see above).
                let ring = unsafe { &*(ring_ptr as *const RingBufferWriter) };

                for slot in out.iter_mut() {
                    let sample = mic_buf.pop_front().unwrap_or(0.0);
                    let gated = if pass { sample } else { 0.0 };
                    ring.push(gated);
                    *slot = 0.0; // don't actually play through the dummy device
                }
            },
            |err| warn!("output stream error: {err}"),
            None,
        )
        .context("failed to build output stream for ring buffer routing")?;

    stream.play().context("failed to start output stream")?;
    info!("Ring-buffer output stream started");
    Ok(stream)
}

// ── BlackHole fallback stream ─────────────────────────────────────────────────

fn start_blackhole_stream(
    audio_rx: Receiver<AudioChunk>,
    gate_state: Arc<AtomicBool>,
    device: &cpal::Device,
) -> Result<cpal::Stream> {
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: 16000,
        buffer_size: cpal::BufferSize::Default,
    };

    let mut mic_buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(4096);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(chunk) = audio_rx.try_recv() {
                    mic_buf.extend(chunk.samples.iter().copied());
                }
                let pass = gate_state.load(Ordering::Relaxed);
                let frame_count = out.len() / 2;
                for i in 0..frame_count {
                    let sample = if pass {
                        mic_buf.pop_front().unwrap_or(0.0)
                    } else {
                        mic_buf.pop_front();
                        0.0
                    };
                    out[i * 2] = sample;
                    out[i * 2 + 1] = sample;
                }
            },
            |err| warn!("BlackHole output stream error: {err}"),
            None,
        )
        .context("failed to build BlackHole output stream")?;

    stream.play().context("failed to start BlackHole stream")?;
    info!("BlackHole output stream started");
    Ok(stream)
}

// ── Device discovery ──────────────────────────────────────────────────────────

pub fn find_blackhole_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    host.output_devices().ok()?.find(|d| {
        d.name()
            .map(|n| n.to_lowercase().contains(BLACKHOLE_KEYWORD))
            .unwrap_or(false)
    })
}

/// Find a real output device, skipping BlackHole and Voce virtual devices.
/// Used to find a speaker for test audio playback.
pub fn find_speaker_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    find_non_virtual_output_device(&host)
        .or_else(|| host.default_output_device())
}

fn find_non_virtual_output_device(host: &cpal::Host) -> Option<cpal::Device> {
    host.output_devices().ok()?.find(|d| {
        d.name()
            .map(|n| {
                let lower = n.to_lowercase();
                !lower.contains("blackhole") &&
                !lower.contains("voce") &&
                !lower.contains("loopback")
            })
            .unwrap_or(false)
    })
}
