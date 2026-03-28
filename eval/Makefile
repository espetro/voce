# eval/Makefile — Voce evaluation harness
#
# Run from the repo root:
#   make -f eval/Makefile eval-setup
#   make -f eval/Makefile eval-run
#
# Prerequisites: uv (https://docs.astral.sh/uv/), Rust toolchain

VOCE_BIN   ?= target/release/voce
SCRIPTS    := eval/scripts
VENV       := eval/.venv
PY         := $(VENV)/bin/python
UV         := uv

.PHONY: eval-setup eval-run eval-run-debug eval-threshold

## Create venv, install Python deps, download LibriSpeech, generate fixtures,
## and build the release binary.
eval-setup: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/download_samples.py
	$(PY) $(SCRIPTS)/make_fixtures.py
	cargo build --release

## Install Python dependencies into the virtual environment.
$(VENV)/pyvenv.cfg: eval/requirements.txt
	$(UV) venv $(VENV)
	$(UV) pip install -r eval/requirements.txt --python $(VENV)
	@touch $@

## Run evaluation against the release binary using the threshold from
## ~/.voce/config.json (or the default 0.75 if no config exists).
eval-run: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/evaluate.py --binary $(VOCE_BIN)

## Same as eval-run but with RUST_LOG=voce=debug so the binary's tracing
## output is visible on stderr alongside the Python report.
eval-run-debug: $(VENV)/pyvenv.cfg
	RUST_LOG=voce=debug $(PY) $(SCRIPTS)/evaluate.py --binary $(VOCE_BIN)

## Sweep the cosine similarity threshold from 0.60 to 0.90 in 0.05 steps.
## Temporarily patches ~/.voce/config.json; restores it afterwards.
## Use this to find the optimal threshold for your enrolled voice.
eval-threshold: $(VENV)/pyvenv.cfg
	@echo "Threshold sweep: 0.60 → 0.90 (step 0.05)"
	@echo "────────────────────────────────────────"
	@orig=$$(cat ~/.voce/config.json 2>/dev/null || echo '{"threshold":0.75,"vote_window":3}'); \
	for t in 0.60 0.65 0.70 0.75 0.80 0.85 0.90; do \
		printf '{"threshold":%s,"vote_window":3}\n' "$$t" > ~/.voce/config.json; \
		echo ""; \
		echo "=== threshold=$$t ==="; \
		$(PY) $(SCRIPTS)/evaluate.py --binary $(VOCE_BIN) || true; \
	done; \
	printf '%s\n' "$$orig" > ~/.voce/config.json; \
	echo ""; \
	echo "Config restored."
