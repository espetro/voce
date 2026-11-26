# Fixtures

Synthetic 16 kHz mono f32 fixtures are generated in-process by
`synthPcm()` (src/audio_io.ts) so tests stay deterministic without
binary blobs in git:

- enrollment: 6 s, seed 1, tone 210 Hz (modulated)
- target: same seed/family as enrollment → similarity ≈ 1.0, passes gate
- other: seed 7, tone 950 Hz → similarity ≈ 0.05-0.07, fails gate

Raw PCM fixture format: little-endian f32 @ 16 kHz mono (`.pcm`).
WAV fixtures: 16-bit PCM, 16 kHz, mono or stereo (`.wav`).
