/// Application state machine.
///
/// Transitions:
/// Idle → ModelLoading → OnboardingReady → Recording(1) → Recording(2)
/// → Adapting → TestReady → ActiveStandby ↔ Filtering
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppState {
    /// Initial state before anything starts.
    Idle,
    /// Downloading / loading ONNX models.
    ModelLoading,
    /// Models ready; awaiting first enrollment recording.
    OnboardingReady,
    /// Recording enrollment sample (1 or 2).
    Recording { index: u8 },
    /// Extracting and averaging embeddings from recordings.
    Adapting,
    /// Enrollment complete; user can run a test.
    TestReady,
    /// Test recording in progress.
    Testing,
    /// Captured audio is playing back through speakers (filtered).
    PlayingBack,
    /// Captured audio is playing back through speakers (unfiltered).
    PlayingBackRaw,
    /// Enrolled and idle — filter running but mic not active.
    ActiveStandby,
    /// Filter actively processing speech.
    #[allow(dead_code)]
    Filtering,
}

impl AppState {
    /// Returns the string identifier sent to the panel JS.
    pub fn as_js_str(&self) -> &'static str {
        match self {
            AppState::Idle => "IDLE",
            AppState::ModelLoading => "MODEL_LOADING",
            AppState::OnboardingReady => "ONBOARDING_READY",
            AppState::Recording { index: 1 } => "RECORDING_1",
            AppState::Recording { .. } => "RECORDING_2",
            AppState::Adapting => "ADAPTING",
            AppState::TestReady => "TEST_READY",
            AppState::Testing => "TESTING",
            AppState::PlayingBack => "PLAYING_BACK",
            AppState::PlayingBackRaw => "PLAYING_BACK_RAW",
            AppState::ActiveStandby => "ACTIVE_STANDBY",
            AppState::Filtering => "FILTERING",
        }
    }
}
