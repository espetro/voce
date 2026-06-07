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

/// A live microphone capture stream at 16000 Hz mono f32.
pub struct CaptureStream {
    _stream: cpal::Stream,
}

impl CaptureStream {
    /// Open the default input device and start sending 512-sample [`AudioChunk`]s.
    ///
    /// Each chunk is sent to `output_tx` (for the BlackHole output callback)
    /// **and** to `inference_tx` (for the inference task), if provided.
    pub fn start(
        output_tx: Sender<AudioChunk>,
        inference_tx: Option<Sender<AudioChunk>>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device found")?;

        info!("Input device: {}", device.name().unwrap_or_default());

        let config = pick_input_config(&device)?;
        info!(
            "Input config: {} Hz, {} ch, {:?}",
            config.sample_rate, config.channels, config.buffer_size
        );

        let seq = Arc::new(AtomicU64::new(0));
        let mut leftover: Vec<f32> = Vec::with_capacity(1024);
        let channels = config.channels as usize;

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Down-mix to mono
                    let mono: Vec<f32> = if channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks_exact(channels)
                            .map(|f| f.iter().sum::<f32>() / channels as f32)
                            .collect()
                    };

                    leftover.extend_from_slice(&mono);

                    while leftover.len() >= 512 {
                        let chunk = AudioChunk {
                            samples: leftover[..512].into(),
                            seq: seq.fetch_add(1, Ordering::Relaxed),
                        };
                        leftover.drain(..512);

                        // Fan-out: send to output stream
                        match output_tx.try_send(chunk.clone()) {
                            Err(TrySendError::Full(_))         => warn!("output_tx full — dropping chunk"),
                            Err(TrySendError::Disconnected(_)) => {} // no BlackHole output, expected
                            Ok(())                             => {}
                        }
                        // Fan-out: send to inference task (optional)
                        if let Some(ref itx) = inference_tx {
                            match itx.try_send(chunk) {
                                Err(TrySendError::Full(_))         => warn!("inference_tx full — dropping chunk"),
                                Err(TrySendError::Disconnected(_)) => {}
                                Ok(())                             => {}
                            }
                        }
                    }
                },
                |err| warn!("input stream error: {err}"),
                None,
            )
            .context("failed to build input stream")?;

        stream.play().context("failed to start input stream")?;
        info!("Capture stream started");
        Ok(CaptureStream { _stream: stream })
    }
}

fn pick_input_config(device: &cpal::Device) -> Result<StreamConfig> {
    const TARGET: u32 = 16000;

    let supported = device
        .supported_input_configs()
        .context("could not query input configs")?;

    let mut best = None;
    for range in supported {
        if range.min_sample_rate() <= TARGET && range.max_sample_rate() >= TARGET {
            best = Some(range);
            break;
        }
    }

    if let Some(range) = best {
        Ok(StreamConfig {
            channels: 1.min(range.channels()).max(1),
            sample_rate: TARGET,
            buffer_size: BufferSize::Default,
        })
    } else {
        warn!("Device does not support {TARGET} Hz — using default. Resampling not implemented.");
        Ok(device.default_input_config().context("no default input config")?.into())
    }
}
