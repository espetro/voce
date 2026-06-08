use crate::app_state::AppState;
use crate::panel::ipc::PanelCmd;

/// All cross-thread messages that funnel through the tao event loop.
#[derive(Debug)]
pub enum AppEvent {
    TrayIcon(tray_icon::TrayIconEvent),
    Menu(muda::MenuEvent),
    StateChanged(AppState),
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
    },
    ModelReady,
    FilterStats {
        similarity: f32,
        passing: bool,
    },
    BlackHoleStatus {
        found: bool,
    },
    DownloadProgress {
        fraction: f32,
    },
    PanelCommand(PanelCmd),
    OpenPanel,
    // Phase 4: profile ready after enrollment
    EnrolledProfileReady(Box<[f32; 256]>),
    // Phase 6: test capture progress / completion / replay
    TestProgress {
        elapsed_s: u32,
    },
    TestCaptureComplete {
        samples: Vec<f32>,
        raw_samples: Vec<f32>,
        voice_pct: f32,
    },
    ReplayTest,
    ReplayTestRaw,
    StopPlayback,
    // Phase 8: filter toggle (emitted by main → panel)
    FilterPaused {
        paused: bool,
    },
}

/// Commands sent from the main thread → inference task.
#[derive(Debug)]
pub enum InferenceCmd {
    StartEnrollment { index: u8 },
    // Phase 5:
    StartFilter { enrolled: Box<[f32; 256]> },
    StopFilter,
    // Phase 6:
    StartTest,
    StopTest,
}
