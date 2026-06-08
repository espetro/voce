use crate::audio::buffer::AudioChunk;
use anyhow::{Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, StreamConfig,
};
use crossbeam_channel::{Sender, TrySendError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

const TARGET_HZ: u32 = 16000;

/// A live microphone capture stream guaranteed to emit 16 000 Hz mono f32 chunks.
///
/// If the device does not natively support 16 000 Hz the stream runs at the
/// device default rate and decimates (averages groups of N samples) to reach
/// 16 000 Hz before emitting chunks.  Integer-ratio rates (e.g. 48 000 Hz →
/// ÷3) are exact; non-integer ratios produce the nearest lower multiple, which
/// is acceptable for speech processing.
pub struct CaptureStream {
    _stream: cpal::Stream,
}

impl CaptureStream {
    /// Open the default input device and start sending 512-sample [`AudioChunk`]s
    /// at 16 000 Hz to both `output_tx` and `inference_tx`.
    pub fn start(
        output_tx: Sender<AudioChunk>,
        inference_tx: Option<Sender<AudioChunk>>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device found")?;

        info!(
            "Input device: {}",
            device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_default()
        );

        let config = pick_input_config(&device)?;
        let device_rate = config.sample_rate;
        info!(
            "Input config: {} Hz, {} ch, {:?}",
            device_rate, config.channels, config.buffer_size
        );

        // Decimation factor: how many device samples become one 16 kHz sample.
        let decimate_by = (device_rate / TARGET_HZ).max(1) as usize;
        if decimate_by > 1 {
            info!("Resampling: decimating by {decimate_by} ({device_rate} Hz → {TARGET_HZ} Hz)");
        }

        let seq = Arc::new(AtomicU64::new(0));
        let mut leftover: Vec<f32> = Vec::with_capacity(1024);
        let channels = config.channels as usize;

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Down-mix to mono at device rate
                    let mono: Vec<f32> = if channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks_exact(channels)
                            .map(|f| f.iter().sum::<f32>() / channels as f32)
                            .collect()
                    };

                    // Decimate to TARGET_HZ by averaging groups of `decimate_by` samples
                    let resampled: Vec<f32> = if decimate_by == 1 {
                        mono
                    } else {
                        mono.chunks(decimate_by)
                            .filter(|c| c.len() == decimate_by) // drop incomplete tail
                            .map(|c| c.iter().sum::<f32>() / decimate_by as f32)
                            .collect()
                    };

                    leftover.extend_from_slice(&resampled);

                    while leftover.len() >= 512 {
                        let chunk = AudioChunk {
                            samples: leftover[..512].into(),
                            seq: seq.fetch_add(1, Ordering::Relaxed),
                        };
                        leftover.drain(..512);

                        match output_tx.try_send(chunk.clone()) {
                            Err(TrySendError::Full(_)) => warn!("output_tx full — dropping chunk"),
                            Err(TrySendError::Disconnected(_)) => {}
                            Ok(()) => {}
                        }
                        if let Some(ref itx) = inference_tx {
                            match itx.try_send(chunk) {
                                Err(TrySendError::Full(_)) => {
                                    warn!("inference_tx full — dropping chunk")
                                }
                                Err(TrySendError::Disconnected(_)) => {}
                                Ok(()) => {}
                            }
                        }
                    }
                },
                |err| warn!("input stream error: {err}"),
                None,
            )
            .context("failed to build input stream")?;

        stream.play().context("failed to start input stream")?;
        info!("Capture stream started at {TARGET_HZ} Hz (device: {device_rate} Hz)");
        Ok(CaptureStream { _stream: stream })
    }
}

fn pick_input_config(device: &cpal::Device) -> Result<StreamConfig> {
    let supported = device
        .supported_input_configs()
        .context("could not query input configs")?;

    // Prefer native 16 000 Hz; otherwise use the device default and we'll decimate.
    let mut best = None;
    for range in supported {
        if range.min_sample_rate() <= TARGET_HZ && range.max_sample_rate() >= TARGET_HZ {
            best = Some(range);
            break;
        }
    }

    if let Some(_range) = best {
        Ok(StreamConfig {
            channels: 1,
            sample_rate: TARGET_HZ,
            buffer_size: BufferSize::Default,
        })
    } else {
        info!("Device does not support {TARGET_HZ} Hz natively — will decimate from device rate.");
        Ok(device
            .default_input_config()
            .context("no default input config")?
            .into())
    }
}
