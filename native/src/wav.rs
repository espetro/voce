//! Minimal WAV reader for the CLI probe.
//!
//! The ABI itself takes raw f32 PCM. This module exists only so a fixture
//! runner can load 16/24/32-bit PCM WAV files and convert them to 16 kHz mono
//! f32 before calling the ABI.

/// Reads a WAV file and returns (interleaved samples, sample rate, channels).
pub fn read_wav(path: &str) -> Result<(Vec<f32>, u32, u16), String> {
    let bytes = std::fs::read(path).map_err(|err| format!("cannot read {path}: {err}"))?;
    parse_wav(&bytes)
}

/// Parses RIFF/WAVE data with 16-bit, 24-bit, or 32-bit PCM/float samples.
pub fn parse_wav(data: &[u8]) -> Result<(Vec<f32>, u32, u16), String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }

    let mut offset = 12usize;
    let mut format: Option<(u16, u16, u32, u16)> = None; // format, channels, rate, bits
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        offset += 8;
        let available = size.min(data.len() - offset);

        match id {
            b"fmt " => {
                if available < 16 {
                    return Err("malformed fmt chunk".into());
                }
                format = Some((
                    u16::from_le_bytes([data[offset], data[offset + 1]]),
                    u16::from_le_bytes([data[offset + 2], data[offset + 3]]),
                    u32::from_le_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]),
                    u16::from_le_bytes([data[offset + 14], data[offset + 15]]),
                ));
            }
            b"data" => {
                let (format_code, channels, sample_rate, bits) =
                    format.ok_or("data chunk before fmt chunk")?;
                if channels == 0 || sample_rate == 0 {
                    return Err("invalid WAV channel count or sample rate".into());
                }
                let bytes_per_sample = usize::from(bits) / 8;
                let frame_bytes = bytes_per_sample * usize::from(channels);
                if frame_bytes == 0 || bits % 8 != 0 {
                    return Err(format!("unsupported bits_per_sample: {bits}"));
                }
                let frames = available / frame_bytes;
                let mut samples = Vec::with_capacity(frames * usize::from(channels));
                for frame in 0..frames {
                    for channel in 0..usize::from(channels) {
                        let start = offset + frame * frame_bytes + channel * bytes_per_sample;
                        let sample = decode_sample(&data[start..start + bytes_per_sample], format_code)?;
                        samples.push(sample);
                    }
                }
                if usize::from(channels) > 1 {
                    let channels_count = usize::from(channels);
                    samples = samples
                        .chunks(channels_count)
                        .map(|frame| frame.iter().sum::<f32>() / channels_count as f32)
                        .collect();
                }
                return Ok((samples, sample_rate, channels));
            }
            _ => {}
        }
        offset += size + (size & 1);
    }
    Err("no data chunk found".into())
}

fn decode_sample(bytes: &[u8], format_code: u16) -> Result<f32, String> {
    match bytes.len() {
        2 => Ok(f64::from(i16::from_le_bytes([bytes[0], bytes[1]])) as f32 / 32768.0),
        3 => {
            let raw = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]) << 8;
            Ok((raw >> 8) as f32 / 8_388_608.0)
        }
        4 if format_code == 3 => Ok(f32::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ])),
        4 => Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0),
        _ => Err(format!("unsupported sample width: {}", bytes.len())),
    }
}

/// Probe-only convenience: linear resample to 16 kHz. Production capture must
/// deliver 16 kHz PCM itself; this helper is never called inside the ABI.
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let out_len = (input.len() as f64 * ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let position = index as f64 / ratio;
        let left = position.floor() as usize;
        let fraction = (position - left as f64) as f32;
        let a = input.get(left).copied().unwrap_or(0.0);
        let b = input.get(left + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * fraction);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_16k_mono_pcm16() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&32_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes());
        bytes.extend_from_slice(&16_384i16.to_le_bytes());

        let (samples, rate, channels) = parse_wav(&bytes).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(rate, 16_000);
        assert_eq!(channels, 1);
        assert!((samples[1] - 0.5).abs() < 1e-6);
    }
}
