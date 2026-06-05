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

MSC_DIR ?= $(HOME)/Downloads/Multi-stream Spontaneous Conversation Training Dataset

.PHONY: eval-setup eval-run eval-run-debug eval-threshold eval-alimeeting-setup eval-alimeeting-run eval-alimeeting-sweep eval-alimeeting-clean eval-msc-setup eval-msc-run eval-msc-sweep eval-msc-clean

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

## Download AliMeeting data, build audio test fixtures, and build binary.
eval-alimeeting-setup: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/download_alimeeting.py
	$(PY) $(SCRIPTS)/make_audio_tests.py
	cargo build --release

## Run AliMeeting audio eval against the release binary.
eval-alimeeting-run: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/eval_audio.py --binary $(VOCE_BIN)

## Sweep cosine similarity threshold 0.50→0.95 for AliMeeting audio eval.
## Shows similarity statistics at each step to find the sweet spot.
eval-alimeeting-sweep: $(VENV)/pyvenv.cfg
	@echo "AliMeeting threshold sweep: 0.50 → 0.95 (step 0.05)"
	@echo "──────────────────────────────────────────────────"
	@orig=$$(cat ~/.voce/config.json 2>/dev/null || echo '{"threshold":0.75,"vote_window":3}'); \
	for t in 0.50 0.55 0.60 0.65 0.70 0.75 0.80 0.85 0.90 0.95; do \
		printf '{"threshold":%s,"vote_window":3}\n' "$$t" > ~/.voce/config.json; \
		echo ""; \
		echo "=== threshold=$$t ==="; \
		$(PY) $(SCRIPTS)/eval_audio.py --binary $(VOCE_BIN) || true; \
	done; \
	printf '%s\n' "$$orig" > ~/.voce/config.json; \
	echo ""; \
	echo "Config restored."

## Remove generated AliMeeting fixtures (keep downloaded data).
eval-alimeeting-clean:
	@echo "Removing generated fixtures (keeping downloaded data)..."
	@rm -f $(SCRIPTS)/../fixtures/mixed_*.wav $(SCRIPTS)/../fixtures/mixed_*.gt.jsonl
	@echo "Fixtures cleaned."

## Generate MSC fixtures and build the release binary.
## Override MSC_DIR if the dataset is in a non-default location:
##   make -f eval/Makefile eval-msc-setup MSC_DIR="/path/to/dataset"
eval-msc-setup: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/make_msc_tests.py --msc-dir "$(MSC_DIR)"
	cargo build --release

## Run MSC audio eval at the threshold in ~/.voce/config.json.
eval-msc-run: $(VENV)/pyvenv.cfg
	$(PY) $(SCRIPTS)/eval_audio.py \
		--binary $(VOCE_BIN) \
		--fixtures-dir eval/fixtures/msc

## Sweep cosine similarity threshold 0.50→0.95 for MSC eval.
eval-msc-sweep: $(VENV)/pyvenv.cfg
	@echo "MSC threshold sweep: 0.50 → 0.95 (step 0.05)"
	@echo "──────────────────────────────────────────────"
	@orig=$$(cat ~/.voce/config.json 2>/dev/null \
	       || echo '{"threshold":0.65,"vote_window":3}'); \
	for t in 0.50 0.55 0.60 0.65 0.70 0.75 0.80 0.85 0.90 0.95; do \
		printf '{"threshold":%s,"vote_window":3}\n' "$$t" > ~/.voce/config.json; \
		echo ""; \
		echo "=== threshold=$$t ==="; \
		$(PY) $(SCRIPTS)/eval_audio.py \
			--binary $(VOCE_BIN) \
			--fixtures-dir eval/fixtures/msc || true; \
	done; \
	printf '%s\n' "$$orig" > ~/.voce/config.json; \
	echo ""; \
	echo "Config restored."

## Remove generated MSC fixtures (keep nothing — dataset is external).
eval-msc-clean:
	@rm -rf eval/fixtures/msc
	@echo "MSC fixtures removed."

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
