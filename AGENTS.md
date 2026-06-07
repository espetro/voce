## Audio architecture

All audio throughout the signal chain is **16 000 Hz mono f32**.

- Capture (`src/audio/capture.rs`): decimates any device sample rate → 16 kHz.
- Processing chunks: 512 samples = 32 ms.
- Inference windows: 48 000 samples = 3 s.
- Output device streams: ring-buffer path uses 48 000 Hz (cpal heartbeat only; audio goes to ring buffer, not the cpal output); BlackHole fallback uses 16 000 Hz (matches mic data rate).

Do not introduce any other sample rate in the processing pipeline.
