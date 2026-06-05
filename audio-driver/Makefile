# audio-driver/Makefile — Build and install VoceAudio.driver
#
# From the repo root:
#   make -C audio-driver          # build only
#   make -C audio-driver install  # build + copy to ~/Library/Audio/Plug-Ins/HAL/
#   make -C audio-driver reload   # install + restart coreaudiod
#
# The compiled .driver is also expected at:
#   target/release/VoceAudio.driver
# so the Rust app can copy it on first launch.

DRIVER_NAME := VoceAudio
BUNDLE      := $(DRIVER_NAME).driver
HAL_DIR     := $(HOME)/Library/Audio/Plug-Ins/HAL
RUST_OUT    := ../target/release/$(BUNDLE)

MACOS_MIN   := 12.0
ARCH        := arm64e x86_64

CFLAGS := \
    -std=c11 \
    -O2 \
    -Wall \
    -Wextra \
    -mmacosx-version-min=$(MACOS_MIN) \
    $(foreach a,$(ARCH),-arch $(a)) \
    -framework CoreAudio \
    -framework Foundation

.PHONY: all bundle install reload clean

all: bundle

## Build the .driver bundle into audio-driver/$(BUNDLE)
bundle: $(BUNDLE)/Contents/MacOS/$(DRIVER_NAME)

$(BUNDLE)/Contents/MacOS/$(DRIVER_NAME): VoceAudio.c Info.plist
	@mkdir -p $(BUNDLE)/Contents/MacOS
	@cp Info.plist $(BUNDLE)/Contents/
	clang $(CFLAGS) \
	    -dynamiclib \
	    -install_name "@rpath/$(DRIVER_NAME)" \
	    -o $@ \
	    VoceAudio.c
	@echo "Built: $@"

## Copy to ~/Library/Audio/Plug-Ins/HAL/ (no admin required)
install: bundle
	@mkdir -p "$(HAL_DIR)"
	@rm -rf "$(HAL_DIR)/$(BUNDLE)"
	@cp -R $(BUNDLE) "$(HAL_DIR)/$(BUNDLE)"
	@echo "Installed: $(HAL_DIR)/$(BUNDLE)"
	@# Also copy to target/release so Rust build.rs can embed it
	@mkdir -p "$(RUST_OUT)"
	@cp -R $(BUNDLE)/. "$(RUST_OUT)/"
	@echo "Copied to: $(RUST_OUT)"

## Install and ask CoreAudio to reload drivers
reload: install
	@echo "Reloading coreaudiod..."
	launchctl kickstart -k system/com.apple.audio.coreaudiod
	@sleep 1.5
	@echo "Done. 'Voce Microphone' should appear in System Settings → Sound."

clean:
	@rm -rf $(BUNDLE)
	@echo "Cleaned."
