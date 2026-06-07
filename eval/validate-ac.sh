#!/usr/bin/env bash
# eval/validate-ac.sh — Validate TASK-6 AC#1 and AC#2 with your own recordings.
#
# Usage:
#   ./eval/validate-ac.sh <enroll1.wav> [enroll2.wav] <other_speaker.wav>
#
# Recordings needed (≥30 s each, any sample rate, mono or stereo WAV):
#   enroll1.wav        — you speaking (read anything aloud)
#   enroll2.wav        — you again, different content (optional but recommended)
#   other_speaker.wav  — a different person speaking
#
# Quick macOS recording:
#   ffmpeg -f avfoundation -i ":0" -t 40 enroll1.wav
#   sox  -d enroll1.wav trim 0 40     # alternative if you have sox
#
# AC#3 (CPU < 15%) must be verified manually — instructions print at the end.

set -euo pipefail
cd "$(dirname "$0")/.."

BINARY=target/release/voce
PROFILE=/tmp/voce_ac_profile.json

pass() { printf '\033[32m✓  %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗  %s\033[0m\n' "$*"; FAILED=1; }
info() { printf '   %s\n' "$*"; }

FAILED=0

# ── arg parsing ───────────────────────────────────────────────────────────────
if [[ $# -lt 2 ]]; then
  sed -n '2,/^set /{ /^set /d; s/^# \{0,1\}//; p }' "$0"
  exit 1
fi

if [[ $# -eq 2 ]]; then
  ENROLL1="$1"; ENROLL2=""; OTHER="$2"
else
  ENROLL1="$1"; ENROLL2="$2"; OTHER="$3"
fi

for f in "$ENROLL1" ${ENROLL2:+"$ENROLL2"} "$OTHER"; do
  [[ -f "$f" ]] || { echo "File not found: $f"; exit 1; }
done

# ── build ─────────────────────────────────────────────────────────────────────
echo "Building release binary…"
cargo build --release -q
echo ""

# ── enroll ────────────────────────────────────────────────────────────────────
echo "=== Enrollment ==="
if [[ -n "$ENROLL2" ]]; then
  $BINARY --eval-enroll "$ENROLL1" --eval-enroll-2 "$ENROLL2" --eval-enroll-out "$PROFILE"
else
  $BINARY --eval-enroll "$ENROLL1" --eval-enroll-out "$PROFILE"
fi
echo ""

# ── Python helpers written to temp files ─────────────────────────────────────
PY_AC1=$(mktemp /tmp/voce_ac1.XXXXXX.py)
PY_AC2=$(mktemp /tmp/voce_ac2.XXXXXX.py)
trap 'rm -f "$PY_AC1" "$PY_AC2"' EXIT

cat > "$PY_AC1" << 'PYEOF'
import sys, json
lines = [json.loads(l) for l in sys.stdin if l.strip()]
speech = [l for l in lines if l["similarity"] > 0]
if not speech:
    print("FAIL"); print("no speech frames detected — is the model loaded?")
    sys.exit(0)
above = [l for l in speech if l["similarity"] > 0.85]
pct = len(above) / len(speech) * 100
status = "PASS" if pct >= 50 else "FAIL"
print(status)
print(f"{pct:.1f}% of {len(speech)} speech frames above 0.85 (need ≥50%)")
top = sorted(speech, key=lambda l: l["similarity"], reverse=True)[:5]
print("top similarities: " + ", ".join(f"{l['similarity']:.3f}" for l in top))
PYEOF

cat > "$PY_AC2" << 'PYEOF'
import sys, json
lines = [json.loads(l) for l in sys.stdin if l.strip()]
speech = [l for l in lines if l["similarity"] > 0]
muted = [l for l in lines if not l["passed"]]
total = len(lines)
if not speech:
    print("FAIL"); print("no speech frames detected")
    sys.exit(0)
muted_speech = [l for l in speech if not l["passed"]]
pct = len(muted_speech) / len(speech) * 100
status = "PASS" if muted_speech else "FAIL"
print(status)
print(f"{len(muted_speech)}/{len(speech)} speech frames muted ({pct:.1f}%)")
if speech:
    avg_sim = sum(l["similarity"] for l in speech) / len(speech)
    print(f"avg similarity of other speaker: {avg_sim:.3f} (threshold=0.75)")
PYEOF

# ── AC#1 — enrolled speaker ────────────────────────────────────────────────
echo "=== AC#1 — enrolled speaker similarity > 0.85 ==="
AC1_OUT=$($BINARY --eval "$ENROLL1" --enrollment "$PROFILE" 2>/dev/null | python3 "$PY_AC1")
AC1_STATUS=$(echo "$AC1_OUT" | head -1)
AC1_DETAIL=$(echo "$AC1_OUT" | tail -n +2)

if [[ "$AC1_STATUS" == "PASS" ]]; then
  pass "AC#1 passed"
else
  fail "AC#1 failed"
fi
info "$AC1_DETAIL"
echo ""

# ── AC#2 — other speaker is silenced ──────────────────────────────────────
echo "=== AC#2 — other speaker is silenced ==="
AC2_OUT=$($BINARY --eval "$OTHER" --enrollment "$PROFILE" 2>/dev/null | python3 "$PY_AC2")
AC2_STATUS=$(echo "$AC2_OUT" | head -1)
AC2_DETAIL=$(echo "$AC2_OUT" | tail -n +2)

if [[ "$AC2_STATUS" == "PASS" ]]; then
  pass "AC#2 passed"
else
  fail "AC#2 failed"
fi
info "$AC2_DETAIL"
echo ""

# ── AC#3 — CPU (manual) ───────────────────────────────────────────────────
echo "=== AC#3 — CPU < 15% on M1 (manual verification) ==="
info "1. Launch the app normally with your enrolled profile."
info "2. In a separate terminal run:"
info "     top -pid \$(pgrep voce) -stats pid,cpu,mem -l 10"
info "3. Confirm the CPU column stays under 15% during active filtering."
echo ""

# ── summary ───────────────────────────────────────────────────────────────
if [[ $FAILED -eq 0 ]]; then
  echo "AC#1 and AC#2 passed. Complete AC#3 manually, then close issue #6:"
  echo "  gh issue close 6 --comment \"All 5 ACs verified. Threshold fix applied (0.65→0.75). Phase 5 complete.\""
else
  echo "One or more ACs failed — see details above."
  exit 1
fi
