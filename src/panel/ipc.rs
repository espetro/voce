use serde::{Deserialize, Serialize};

/// Commands sent from the panel JS → Rust via `window.ipc.postMessage(JSON)`.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum PanelCmd {
    StartRecording { index: u8 },
    StartTest,
    StopTest,
    ReplayTest,
    ReplayTestRaw,
    StopPlayback,
    ConfirmEnrollment,
    Reenroll,
    OpenBlackholeLink,
    // Phase 8: filter and noise suppression toggles
    ToggleFilter,
    SetNoiseSuppression { enabled: bool },
    // Driver setup: background driver installation
    InstallDriver,
    // Reset flow: full reset (wipe config + driver)
    FullReset,
    // Driver setup: open system sound settings
    OpenSystemSound,
}

/// Events sent from Rust → panel JS via `webview.evaluate_script(...)`.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PanelEvent {
    StateChanged {
        state: &'static str,
    },
    RecordingProgress {
        index: u8,
        elapsed_s: u32,
        speech_s: u32,
    },
    RecordingComplete {
        index: u8,
        speech_s: u32,
    },
    RecordingInvalid {
        index: u8,
        reason: &'static str,
    },
    TestProgress {
        elapsed_s: u32,
    },
    FilterStats {
        similarity: f32,
        is_passing: bool,
    },
    BlackholeStatus {
        found: bool,
    },
    DownloadProgress {
        fraction: f32,
    },
    TestStats {
        voice_pct: f32,
    },
    // Phase 8
    FilterPaused {
        paused: bool,
    },
    NoiseSuppression {
        enabled: bool,
    },
    // Driver setup: status update from background install task
    DriverStatus {
        installed: bool,
        device_found: bool,
    },
    // Reset flow: full reset task complete
    ResetComplete {
        success: bool,
    },
}

impl PanelEvent {
    /// Produce a JS `evaluate_script` call string:
    ///   `window.__voce_update(<json-string-literal>)`
    ///
    /// The event is serialised to JSON, then that JSON string is itself
    /// JSON-encoded to produce a valid JS string literal argument.
    /// This handles all Unicode escapes (including U+2028/U+2029) correctly.
    pub fn to_js_call(&self) -> anyhow::Result<String> {
        let payload = serde_json::to_string(self)?;
        Ok(format!(
            "window.__voce_update({})",
            serde_json::to_string(&payload)?
        ))
    }
}

/// Parse a raw IPC body string into a `PanelCmd`.
pub fn parse_cmd(body: &str) -> anyhow::Result<PanelCmd> {
    serde_json::from_str(body).map_err(Into::into)
}
