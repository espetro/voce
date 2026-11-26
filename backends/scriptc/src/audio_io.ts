// Minimal WAV (16-bit PCM mono/stereo) and raw f32 PCM loading.

export interface LoadedAudio {
  pcm: Float32Array; // 16 kHz mono f32
  sampleRate: number;
}

/** Parse a RIFF/WAVE file bytes; resample is NOT done (expect 16 kHz input). */
export function loadWavBytes(bytes: Uint8Array): LoadedAudio {
  if (bytes.length < 44) throw new Error("wav too small");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (String.fromCharCode(bytes[0], bytes[1], bytes[2], bytes[3]) !== "RIFF") {
    throw new Error("not RIFF");
  }
  if (String.fromCharCode(bytes[8], bytes[9], bytes[10], bytes[11]) !== "WAVE") {
    throw new Error("not WAVE");
  }
  let pos = 12;
  let fmt: { channels: number; rate: number; bits: number } | null = null;
  let dataOff = -1, dataLen = 0;
  while (pos + 8 <= bytes.length) {
    const id = String.fromCharCode(bytes[pos], bytes[pos+1], bytes[pos+2], bytes[pos+3]);
    const size = view.getUint32(pos + 4, true);
    if (id === "fmt ") {
      const channels = view.getUint16(pos + 10, true);
      const rate = view.getUint32(pos + 12, true);
      const bits = view.getUint16(pos + 22, true);
      fmt = { channels, rate, bits };
    } else if (id === "data") {
      dataOff = pos + 8;
      dataLen = Math.min(size, bytes.length - dataOff);
    }
    pos += 8 + size + (size & 1);
  }
  if (!fmt || dataOff < 0) throw new Error("wav missing fmt/data");
  if (fmt.rate !== 16000) throw new Error("expected 16 kHz wav, got " + fmt.rate);
  if (fmt.bits !== 16) throw new Error("expected 16-bit wav, got " + fmt.bits + "-bit");

  const frames = Math.floor(dataLen / (2 * fmt.channels));
  const pcm = new Float32Array(frames);
  for (let i = 0; i < frames; i++) {
    // Downmix: average channels.
    let sum = 0;
    for (let c = 0; c < fmt!.channels; c++) {
      sum += view.getInt16(dataOff + (i * fmt!.channels + c) * 2, true);
    }
    pcm[i] = sum / fmt!.channels / 32768;
  }
  return { pcm, sampleRate: fmt.rate };
}

/** Raw little-endian f32 PCM at 16 kHz mono. */
export function loadPcmBytes(bytes: Uint8Array): LoadedAudio {
  const frames = Math.floor(bytes.length / 4);
  const pcm = new Float32Array(frames);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < frames; i++) pcm[i] = view.getFloat32(i * 4, true);
  return { pcm, sampleRate: 16000 };
}

/** Synthesize deterministic 16 kHz mono f32 audio for fixtures. */
export function synthPcm(seconds: number, seedAmp: number, freqHz: number): Float32Array {
  const n = seconds * 16000;
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    // Deterministic pseudo-content: tone + a seeded slow modulation.
    const t = i / 16000;
    const mod = 0.5 + 0.5 * Math.sin(2 * Math.PI * (seedAmp * 0.013) * t);
    out[i] = seedAmp * 0.3 * mod * Math.sin(2 * Math.PI * freqHz * t);
  }
  return out;
}
