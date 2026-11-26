// Native-backed scriptc probe. Same WAV/PCM inputs as src/cli.ts, but all
// enrollment, scoring and profile state live in the real voce_native engine.
//
// Usage (built executable):
//   voce-scriptc-native run --input file.wav --enroll enroll.wav
//   voce-scriptc-native run --input file.pcm --profile native-profile.json
//   voce-scriptc-native bench [--seconds N] [--repeat 2]
//   voce-scriptc-native roundtrip --enroll enroll.wav --input file.wav
//
// Build:
//   ffi/build-wrapper.sh
//   flock /tmp/tsc.lock scriptc build src/cli_native.ts \
//     --ffi ffi/voce_native_scriptc.json -o voce-scriptc-native --no-keep-llvm

import { readFileSync, readlinkSync, statSync, writeFileSync } from "node:fs";
import process from "node:process";
import { VoceNativeAdapter } from "./voce_native_adapter.ts";
import { loadPcmBytes, loadWavBytes, synthPcm } from "./audio_io.ts";
import { type VoceNativeWindowScore, WINDOW_SAMPLES } from "./types.ts";

interface NativeWindow {
  index: number;
  samples: number;
  similarity: number;
  passed: boolean;
  runtime_ms: number;
  speech_probability: number;
  decision: number;
}

interface ScoredInput {
  label: string;
  samples: number;
  window_count: number;
  passed_window_count: number;
  windows: NativeWindow[];
  score_time_ms: number;
  mean_window_time_ms: number;
  wall_ms: number;
}

function rssKib(): number | null {
  try {
    const txt = readFileSync("/proc/self/status", "utf8");
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(txt);
    if (m) return parseInt(m[1], 10);
  } catch {}
  return null;
}

function artifactBytes(): number | null {
  try {
    return statSync(readlinkSync("/proc/self/exe")).size;
  } catch {}
  return null;
}

function loadInput(path: string): Float32Array {
  const bytes = readFileSync(path);
  if (path.endsWith(".wav")) return loadWavBytes(bytes).pcm;
  return loadPcmBytes(bytes).pcm;
}

function getOpt(args: string[], name: string): string | null {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : null;
}

function getInt(args: string[], name: string, fallback: number): number {
  const raw = getOpt(args, name);
  return raw === null ? fallback : parseInt(raw, 10);
}

function windows(scores: VoceNativeWindowScore[]): NativeWindow[] {
  return scores.map((score, index) => ({
    index,
    samples: WINDOW_SAMPLES,
    similarity: score.similarity,
    passed: score.decision === 1,
    runtime_ms: score.runtime_ms,
    speech_probability: score.speech_probability,
    decision: score.decision,
  }));
}

/** Runtime is measured by the native engine, so it is excluded from digests. */
function stableWindows(values: NativeWindow[]): unknown {
  return values.map((w) => [w.index, w.samples, w.similarity, w.passed, w.decision]);
}

function enrollAdapter(adapter: VoceNativeAdapter, pcm: Float32Array): string {
  adapter.resetEnrollment();
  adapter.enrollPcm(pcm);
  adapter.finishEnrollment();
  return adapter.serializeProfileJson();
}

function scoreInput(adapter: VoceNativeAdapter, pcm: Float32Array, label: string): ScoredInput {
  const t0 = Date.now();
  const scores = adapter.processPcm(pcm);
  const scored = windows(scores);
  let scoreTime = 0;
  for (let i = 0; i < scored.length; i++) scoreTime += scored[i].runtime_ms;
  return {
    label,
    samples: pcm.length,
    window_count: scored.length,
    passed_window_count: scored.filter((w) => w.passed).length,
    windows: scored,
    score_time_ms: scoreTime,
    mean_window_time_ms: scored.length > 0 ? scoreTime / scored.length : 0,
    wall_ms: Date.now() - t0,
  };
}

function main(): void {
  const t0 = Date.now();
  const argv = process.argv.slice(2);
  if (argv[0] === "scriptc") argv.shift(); // built exe prepends a marker arg
  const cmd = argv[0] ?? "run";
  let out: unknown;

  if (cmd === "run") {
    const input = getOpt(argv, "--input");
    if (!input) throw new Error("run: --input required");
    const adapter = new VoceNativeAdapter();
    const enroll = getOpt(argv, "--enroll");
    const profile = getOpt(argv, "--profile");
    let profileJson: string | null = null;
    if (profile !== null) {
      profileJson = readFileSync(profile, "utf8");
      adapter.deserializeProfileJson(profileJson);
    } else if (enroll !== null) {
      profileJson = enrollAdapter(adapter, loadInput(enroll));
    } else {
      throw new Error("run: --enroll or --profile required");
    }
    const profileOut = getOpt(argv, "--out-profile");
    if (profileOut !== null && profileJson !== null) {
      writeFileSync(profileOut, new TextEncoder().encode(profileJson));
    }
    const threshold = getOpt(argv, "--threshold");
    if (threshold !== null) adapter.setThreshold(parseFloat(String(threshold)));
    const scored = scoreInput(adapter, loadInput(input), input);
    adapter.close();
    out = {
      command: "run", backend: adapter.backendName, input,
      embedding_dim: adapter.embeddingDim, enrolled: true,
      profile_json: profileJson === null ? null : JSON.parse(profileJson),
      profile_out: profileOut,
      samples: scored.samples,
      window_count: scored.window_count,
      passed_window_count: scored.passed_window_count,
      windows: scored.windows,
      score_time_ms: scored.score_time_ms,
      mean_window_time_ms: scored.mean_window_time_ms,
      score_wall_ms: scored.wall_ms,
      wall_ms: Date.now() - t0, rss_kib: rssKib(),
      artifact_bytes: artifactBytes(),
    };
  } else if (cmd === "bench") {
    const secs = parseFloat(getOpt(argv, "--seconds") ?? "10");
    const repeat = getInt(argv, "--repeat", 2);
    const enrollFile = getOpt(argv, "--enroll");
    const targetFile = getOpt(argv, "--input");
    const otherFile = getOpt(argv, "--other");
    const runs = [];
    for (let i = 0; i < repeat; i++) {
      const adapter = new VoceNativeAdapter();
      const profileJson = enrollAdapter(
        adapter, enrollFile !== null ? loadInput(enrollFile) : synthPcm(6, 1, 210));
      const target = scoreInput(
        adapter, targetFile !== null ? loadInput(targetFile) : synthPcm(secs, 1, 210),
        targetFile ?? "target");
      const other = scoreInput(
        adapter, otherFile !== null ? loadInput(otherFile) : synthPcm(secs, 7, 950),
        otherFile ?? "other");
      adapter.close();
      runs.push({
        profile_json: JSON.parse(profileJson),
        profile_digest: profileJson,
        target_digest: stableWindows(target.windows),
        other_digest: stableWindows(other.windows),
        target, other,
      });
    }
    let deterministic = runs.length > 1;
    for (let i = 1; i < runs.length; i++) {
      const a = runs[0], b = runs[i];
      if (a.profile_digest !== b.profile_digest ||
          JSON.stringify(a.target_digest) !== JSON.stringify(b.target_digest) ||
          JSON.stringify(a.other_digest) !== JSON.stringify(b.other_digest)) {
        deterministic = false;
      }
    }
    out = {
      command: "bench", backend: "voce_native-staticlib", seconds: secs,
      repeat, deterministic, enroll: enrollFile, target: targetFile,
      other: otherFile, runs, wall_ms: Date.now() - t0,
      rss_kib: rssKib(), artifact_bytes: artifactBytes(),
    };
  } else if (cmd === "roundtrip") {
    const input = getOpt(argv, "--input");
    const enroll = getOpt(argv, "--enroll");
    if (!input || !enroll) throw new Error("roundtrip: --input and --enroll required");
    const adapter = new VoceNativeAdapter();
    const json = enrollAdapter(adapter, loadInput(enroll));
    const first = scoreInput(adapter, loadInput(input), "before");
    adapter.close();
    const restored = new VoceNativeAdapter();
    restored.deserializeProfileJson(json);
    const second = scoreInput(restored, loadInput(input), "after");
    restored.close();
    out = {
      command: "roundtrip", backend: "voce_native-staticlib", input, enroll,
      profile_bytes: json.length,
      profile_json: JSON.parse(json),
      score_digest_equal:
        JSON.stringify(stableWindows(first.windows)) ===
        JSON.stringify(stableWindows(second.windows)),
      before: first, after: second,
      wall_ms: Date.now() - t0, rss_kib: rssKib(), artifact_bytes: artifactBytes(),
    };
  } else {
    throw new Error("unknown command: " + cmd);
  }
  console.log(JSON.stringify(out, null, 2));
}

main();
