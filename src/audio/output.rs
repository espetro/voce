use crate::audio::buffer::AudioChunk;
use anyhow::{Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, StreamConfig,
};
use crossbeam_channel::Receiver;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{info, warn};

pub const BLACKHOLE_KEYWORD: &str = "blackhole";

/// A live output stream to the BlackHole 2ch virtual device.
/// Keeping the returned value alive keeps the stream open; dropping it stops output.
pub struct OutputStream {
    _stream: cpal::Stream,
}

impl OutputStream {
    /// Find BlackHole in the output device list and open an output stream.
    ///
    /// The output callback:
    /// - pulls pending `AudioChunk`s from `audio_rx` into a local ring buffer
    /// - reads `gate_state` (AtomicBool: true = pass, false = mute)
    /// - copies mic samples (or zeros) to the stereo output buffer
    pub fn start(audio_rx: Receiver<AudioChunk>, gate_state: Arc<AtomicBool>) -> Result<Self> {
        let device = find_blackhole_device()
            .context("BlackHole 2ch not found — install from https://existential.audio/blackhole/")?;

        info!("Output device: {}", device.name().unwrap_or_default());

        let config = StreamConfig {
            channels: 2, // BlackHole 2ch is stereo
            sample_rate: 22050,
            buffer_size: BufferSize::Default,
        };

        // Local sample ring buffer: holds mic samples ready to be written to output.
        // Using a VecDeque to allow cheap pop_front without shifting.
        // Pre-allocated to avoid heap allocs inside the real-time callback.
        let mut mic_buf: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(4096);

        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // Drain pending mic chunks into mic_buf
                    while let Ok(chunk) = audio_rx.try_recv() {
                        mic_buf.extend(chunk.samples.iter().copied());
                    }

                    let pass = gate_state.load(Ordering::Relaxed);
                    let frame_count = out.len() / 2; // stereo

                    for i in 0..frame_count {
                        let sample = if pass {
                            mic_buf.pop_front().unwrap_or(0.0)
                        } else {
                            mic_buf.pop_front(); // consume but discard
                            0.0
                        };
                        // Duplicate mono sample to both L and R channels
                        out[i * 2] = sample;
                        out[i * 2 + 1] = sample;
                    }
                },
                |err| warn!("output stream error: {err}"),
                None,
            )
            .context("failed to build BlackHole output stream")?;

        stream.play().context("failed to start output stream")?;
        info!("BlackHole output stream started");
        Ok(OutputStream { _stream: stream })
    }

    /// Returns the BlackHole device if found in the system output device list.
    pub fn find_blackhole_device() -> Option<cpal::Device> {
        find_blackhole_device()
    }

    /// Returns true if a BlackHole-named output device exists.
    pub fn detect_blackhole() -> bool {
        find_blackhole_device().is_some()
    }
}

fn find_blackhole_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    match host.output_devices() {
        Ok(devices) => devices
            .filter(|d| {
                d.name()
                    .map(|n| n.to_lowercase().contains(BLACKHOLE_KEYWORD))
                    .unwrap_or(false)
            })
            .next(),
        Err(e) => {
            warn!("Could not enumerate output devices: {e}");
            None
        }
    }
}
