// scriptc backend spike CLI.
//
// Usage (built executable):
//   voce-scriptc bench [--seconds N]              determinism + metrics
//   voce-scriptc enroll --out profile.json        synth enrollment, write profile
//   voce-scriptc run --input file.wav --profile profile.json
//   voce-scriptc run --input file.pcm  --profile profile.json
//
// Emits JSON metrics on stdout: embedding dim, per-window similarity/pass,
// runtime per window, RSS (VmHWM), wall time, and bench determinism.

import { readFileSync, writeFileSync } from "node:fs";
import process from "node:process";
import { Engine } from "./engine.ts";
import { MockShim } from "./mock_shim.ts";
import { loadPcmBytes, loadWavBytes, synthPcm } from "./audio_io.ts";
import type { Profile } from "./types.ts";

function validateProfile(v: unknown): Profile {
  if (typeof v !== "object" || v === null) throw new Error("profile: not an object");
  const o = v as { version?: unknown; embedding_dim?: unknown; threshold?: unknown; vote_window?: unknown; window_count?: unknown; embedding?: unknown };
  if (o.version !== 1) throw new Error("profile: bad version");
  if (typeof o.embedding_dim !== "number" || typeof o.threshold !== "number" ||
      typeof o.vote_window !== "number" || typeof o.window_count !== "number") {
    throw new Error("profile: bad numeric field");
  }
  if (!Array.isArray(o.embedding) || !o.embedding.every((x) => typeof x === "number")) {
    throw new Error("profile: bad embedding");
  }
  return {
    version: 1,
    embedding_dim: o.embedding_dim,
    embedding: o.embedding,
    window_count: o.window_count,
    threshold: o.threshold,
    vote_window: o.vote_window,
  };
}

function rssKib(): number | null {
  try {
    const txt = readFileSync("/proc/self/status", "utf8");
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(txt);
    if (m) return parseInt(m[1], 10);
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

function runBenchPass(secs: number): unknown {
  const engine = new Engine(new MockShim());
  const enrollSecs = 6;
  engine.addEnrollmentPcm(synthPcm(enrollSecs, 1, 210));
  const p = engine.finishEnrollment();
  if (!p) throw new Error("bench: enrollment failed");
  const target = synthPcm(secs, 1, 210);   // enrolled content
  const other = synthPcm(secs, 7, 950);    // non-enrolled content
  const mTarget = engine.processPcm(target);
  const engine2 = new Engine(new MockShim());
  engine2.loadProfile(p);
  const mOther = engine2.processPcm(other);
  return {
    profile_embedding_dim: p.embedding_dim,
    target: mTarget,
    other: mOther,
  };
}

function main(): void {
  const t0 = Date.now();
  const argv = process.argv.slice(2);
  if (argv[0] === "scriptc") argv.shift(); // built exe prepends a marker arg
  const cmd = argv[0] ?? "bench";

  let out: unknown;
  if (cmd === "enroll") {
    const engine = new Engine(new MockShim());
    const seed = parseInt(getOpt(argv, "--seed") ?? "1", 10);
    const secs = parseFloat(getOpt(argv, "--seconds") ?? "6");
    engine.addEnrollmentPcm(synthPcm(secs, seed, 180 + seed * 30));
    const p = engine.finishEnrollment();
    if (!p) throw new Error("enrollment produced no profile");
    const path = getOpt(argv, "--out") ?? "profile.json";
    writeFileSync(path, new TextEncoder().encode(JSON.stringify(p)));
    out = { command: "enroll", profile_path: path, embedding_dim: p.embedding_dim,
            window_count: p.window_count, wall_ms: Date.now() - t0, rss_kib: rssKib() };
  } else if (cmd === "run") {
    const input = getOpt(argv, "--input");
    if (!input) throw new Error("run: --input required");
    const profPath = getOpt(argv, "--profile") ?? "profile.json";
    const engine = new Engine(new MockShim());
    engine.loadProfile(validateProfile(JSON.parse(readFileSync(profPath, "utf8"))));
    const pcm = loadInput(input);
    const m = engine.processPcm(pcm);
    out = { command: "run", input, ...m, wall_ms: Date.now() - t0 };
  } else {
    const secs = parseFloat(getOpt(argv, "--seconds") ?? "10");
    const passA = runBenchPass(secs);
    const passB = runBenchPass(secs);
    const deterministic = JSON.stringify(passA) === JSON.stringify(passB);
    out = { command: "bench", seconds: secs, deterministic, pass1: passA, pass2: passB,
            wall_ms: Date.now() - t0, rss_kib: rssKib() };
  }
  console.log(JSON.stringify(out, null, 2));
}

main();
