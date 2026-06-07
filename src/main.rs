mod app_state;
mod audio;
mod config;
mod driver;
mod enrollment;
mod eval;
mod events;
mod filter;
mod inference;
mod model;
mod panel;

use app_state::AppState;
use audio::{buffer::AudioChunk, capture::CaptureStream, denoise::Denoiser, output::OutputStream};
use events::{AppEvent, InferenceCmd};
use panel::ipc::{PanelCmd, PanelEvent, parse_cmd};

use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tracing::{error, info, warn};
use clap::Parser;
use std::path::PathBuf;

type TokioHandle = tokio::runtime::Handle;

use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId, WindowLevel},
};

// Panel: SolidJS app bundled to a single self-contained HTML by vite-plugin-singlefile.
// Build first with: just build-panel
const PANEL_HTML: &str = include_str!("../assets/panel/dist/index.html");

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

struct SharedState {
    app_state: AppState,
    blackhole_found: bool,
    last_similarity: f32,
    models: Option<model::ModelSet>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            app_state: AppState::Idle,
            blackhole_found: false,
            last_similarity: 0.0,
            models: None,
        }
    }
}

// ---------------------------------------------------------------------------
// VoceApp
// ---------------------------------------------------------------------------

struct VoceApp {
    shared: Arc<Mutex<SharedState>>,
    proxy: EventLoopProxy<AppEvent>,
    tokio: TokioHandle,
    tray_icon: Option<tray_icon::TrayIcon>,
    status_item: Option<muda::MenuItem>,
    filter_toggle_item: Option<muda::MenuItem>,
    panel_window: Option<Window>,
    webview: Option<wry::WebView>,
    panel_open: bool,

    gate_state: Arc<AtomicBool>,
    filter_paused: Arc<AtomicBool>,
    noise_suppression: Arc<AtomicBool>,
    // BlackHole output channel
    audio_tx: Sender<AudioChunk>,
    audio_rx: Option<Receiver<AudioChunk>>,
    // Inference audio feed
    inference_audio_tx: Sender<AudioChunk>,
    inference_audio_rx: Option<Receiver<AudioChunk>>,
    // Inference command channel
    inference_cmd_tx: Sender<InferenceCmd>,
    inference_cmd_rx: Option<Receiver<InferenceCmd>>,

    _capture: Option<CaptureStream>,
    _output: Option<OutputStream>,

    last_test_samples: Option<Vec<f32>>,
    last_test_raw_samples: Option<Vec<f32>>,
    playback_stop: Arc<AtomicBool>,
}

impl VoceApp {
    fn new(proxy: EventLoopProxy<AppEvent>, tokio: TokioHandle) -> Self {
        let (audio_tx, audio_rx)             = bounded::<AudioChunk>(64);
        let (inf_audio_tx, inf_audio_rx)     = bounded::<AudioChunk>(64);
        let (inf_cmd_tx, inf_cmd_rx)         = bounded::<InferenceCmd>(32);
        let cfg = config::Config::load(&config::config_path()).unwrap_or_default();
        Self {
            shared: Arc::new(Mutex::new(SharedState::new())),
            proxy,
            tokio,
            tray_icon: None,
            status_item: None,
            filter_toggle_item: None,
            panel_window: None,
            webview: None,
            panel_open: false,
            gate_state: Arc::new(AtomicBool::new(true)),
            filter_paused: Arc::new(AtomicBool::new(false)),
            noise_suppression: Arc::new(AtomicBool::new(cfg.noise_suppression)),
            audio_tx,
            audio_rx: Some(audio_rx),
            inference_audio_tx: inf_audio_tx,
            inference_audio_rx: Some(inf_audio_rx),
            inference_cmd_tx: inf_cmd_tx,
            inference_cmd_rx: Some(inf_cmd_rx),
            _capture: None,
            _output: None,
            last_test_samples: None,
            last_test_raw_samples: None,
            playback_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start_audio(&mut self) {
        let rx = match self.audio_rx.take() {
            Some(r) => r,
            None => { warn!("Audio already started"); return; }
        };

        match CaptureStream::start(self.audio_tx.clone(), Some(self.inference_audio_tx.clone())) {
            Ok(s) => { self._capture = Some(s); info!("Microphone capture running"); }
            Err(e) => {
                error!("Failed to start microphone capture: {e}");
                let (tx2, rx2) = bounded::<AudioChunk>(64);
                self.audio_tx = tx2;
                self.audio_rx = Some(rx2);
                return;
            }
        }

        match OutputStream::start(rx, self.gate_state.clone()) {
            Ok(s) => {
                self._output = Some(s);
                info!("BlackHole output running");
                let _ = self.proxy.send_event(AppEvent::BlackHoleStatus { found: true });
            }
            Err(e) => {
                warn!("BlackHole output not started: {e}");
                let _ = self.proxy.send_event(AppEvent::BlackHoleStatus { found: false });
            }
        }
    }

    /// Create the panel Window + WebView if they don't exist yet.
    fn ensure_panel(&mut self, event_loop: &ActiveEventLoop) {
        if self.panel_window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Voce")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(320u32, 480u32));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(e) => { error!("Failed to create panel window: {e}"); return; }
        };

        let proxy_ipc = self.proxy.clone();
        let webview = wry::WebViewBuilder::new()
            .with_html(PANEL_HTML)
            .with_ipc_handler(move |req: wry::http::Request<String>| {
                let body = req.body();
                match parse_cmd(body) {
                    Ok(cmd) => { let _ = proxy_ipc.send_event(AppEvent::PanelCommand(cmd)); }
                    Err(e) => warn!("IPC parse error: {e} — body: {body}"),
                }
            })
            .with_devtools(cfg!(debug_assertions))
            .build(&window);

        match webview {
            Ok(wv) => {
                self.webview = Some(wv);
                self.panel_window = Some(window);
                info!("Panel WebView created");
            }
            Err(e) => error!("Failed to create WebView: {e}"),
        }
    }

    /// Send a `PanelEvent` to the JS layer via `evaluate_script`.
    fn send_to_panel(&self, event: &PanelEvent) {
        if let (Some(wv), true) = (&self.webview, self.panel_open) {
            match event.to_js_call() {
                Ok(js) => { if let Err(e) = wv.evaluate_script(&js) { warn!("evaluate_script error: {e}"); } }
                Err(e) => warn!("PanelEvent serialise error: {e}"),
            }
        }
    }

    /// Position and show the panel anchored below the tray icon rect.
    fn show_panel(&mut self, tray_rect: &tray_icon::Rect) {
        let Some(window) = &self.panel_window else { return };

        let panel_w: f64 = 320.0;
        // Centre horizontally on the tray icon; open below the menubar
        let x = tray_rect.position.x + tray_rect.size.width as f64 / 2.0 - panel_w / 2.0;
        let y = tray_rect.position.y + tray_rect.size.height as f64 + 4.0;

        window.set_outer_position(PhysicalPosition::new(x, y));
        window.set_visible(true);
        self.panel_open = true;

        // Push the current state to the panel so it shows the right screen
        let state_str = self.shared.lock().unwrap().app_state.as_js_str();
        self.send_to_panel(&PanelEvent::StateChanged { state: state_str });

        // Push BlackHole status
        let found = self.shared.lock().unwrap().blackhole_found;
        self.send_to_panel(&PanelEvent::BlackholeStatus { found });

        // Push filter and noise suppression state
        let paused = self.filter_paused.load(Ordering::Relaxed);
        self.send_to_panel(&PanelEvent::FilterPaused { paused });
        let ns_enabled = self.noise_suppression.load(Ordering::Relaxed);
        self.send_to_panel(&PanelEvent::NoiseSuppression { enabled: ns_enabled });
    }

    fn hide_panel(&mut self) {
        if let Some(w) = &self.panel_window {
            w.set_visible(false);
        }
        self.panel_open = false;
    }
}

impl ApplicationHandler<AppEvent> for VoceApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause != StartCause::Init { return; }

        info!("Voce starting up");
        event_loop.set_control_flow(ControlFlow::Wait);

        // Tray icon
        let icon = load_tray_icon_loading();
        let (menu, status_item, filter_toggle_item) = build_tray_menu();
        self.status_item = Some(status_item);
        self.filter_toggle_item = Some(filter_toggle_item);
        match tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Voce — loading…")
            .with_icon(icon)
            .build()
        {
            Ok(tray) => { self.tray_icon = Some(tray); info!("Tray icon created"); }
            Err(e) => error!("Failed to create tray icon: {e}"),
        }

        let proxy_t = self.proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = proxy_t.send_event(AppEvent::TrayIcon(e));
        }));
        let proxy_m = self.proxy.clone();
        muda::MenuEvent::set_event_handler(Some(move |e| {
            let _ = proxy_m.send_event(AppEvent::Menu(e));
        }));

        // Pre-create the panel so it's ready before first click
        self.ensure_panel(event_loop);

        // Start audio
        self.start_audio();

        // Model loading
        let proxy_bg = self.proxy.clone();
        let shared_bg = self.shared.clone();
        let models_dir = config::models_dir();
        self.tokio.spawn(async move {
            let _ = proxy_bg.send_event(AppEvent::StateChanged(AppState::ModelLoading));
            let proxy_prog = proxy_bg.clone();
            let result = model::ModelSet::load(&models_dir, move |f| {
                let _ = proxy_prog.send_event(AppEvent::DownloadProgress { fraction: f });
            })
            .await;
            match result {
                Ok(models) => {
                    info!("Models loaded successfully");
                    shared_bg.lock().unwrap().models = Some(models);
                    let _ = proxy_bg.send_event(AppEvent::ModelReady);
                }
                Err(e) => error!("Model loading failed: {e:#}"),
            }
        });
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Focused(false) => self.hide_panel(),
            WindowEvent::CloseRequested => self.hide_panel(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            // ---- Model ready ----
            AppEvent::ModelReady => {
                info!("Models ready");

                // Take models out of shared state and hand them to the inference task
                let models = self.shared.lock().unwrap().models.take();
                if let (Some(m), Some(audio_rx), Some(cmd_rx)) = (
                    models,
                    self.inference_audio_rx.take(),
                    self.inference_cmd_rx.take(),
                ) {
                    let gate   = self.gate_state.clone();
                    let paused = self.filter_paused.clone();
                    let prx2   = self.proxy.clone();
                    let ns_flag = self.noise_suppression.clone();
                    match Denoiser::new(ns_flag) {
                        Ok(denoiser) => {
                            self.tokio.spawn(inference::run(m, audio_rx, cmd_rx, gate, paused, denoiser, prx2));
                            info!("Inference task spawned (with denoiser)");
                        }
                        Err(e) => {
                            error!("Failed to create denoiser: {e} — inference task not started");
                        }
                    }
                }

                // Check for an existing enrolled profile (returning user)
                let profile_path = config::enrolled_embedding_path();
                if profile_path.exists() {
                    match enrollment::profile::VoiceProfile::load(&profile_path) {
                        Ok(profile) => {
                            if let Some(arr) = profile.as_array() {
                                info!("Existing voice profile found — starting filter");
                                let _ = self.inference_cmd_tx.send(InferenceCmd::StartFilter {
                                    enrolled: Box::new(arr),
                                });
                                let _ = self.proxy.send_event(AppEvent::StateChanged(AppState::ActiveStandby));
                            }
                        }
                        Err(e) => warn!("Could not load existing profile: {e}"),
                    }
                } else {
                    // First launch — show onboarding
                    let _ = self.proxy.send_event(AppEvent::StateChanged(AppState::OnboardingReady));
                    let _ = self.proxy.send_event(AppEvent::OpenPanel);
                }

                if let Some(tray) = &self.tray_icon {
                    let _ = tray.set_tooltip(Some("Voce — ready"));
                    let _ = tray.set_icon(Some(load_tray_icon_idle()));
                }
            }

            // ---- Open panel (auto on first launch) ----
            AppEvent::OpenPanel => {
                // Use a dummy rect centred on the screen for auto-open
                // (real position will be set on tray click from now on)
                if !self.panel_open {
                    if let Some(w) = &self.panel_window {
                        // Position near top-right as a reasonable default
                        w.set_outer_position(PhysicalPosition::new(1400.0f64, 30.0));
                        w.set_visible(true);
                        w.focus_window();
                        self.panel_open = true;
                        let state_str = self.shared.lock().unwrap().app_state.as_js_str();
                        self.send_to_panel(&PanelEvent::StateChanged { state: state_str });
                        let found = self.shared.lock().unwrap().blackhole_found;
                        self.send_to_panel(&PanelEvent::BlackholeStatus { found });
                        let paused = self.filter_paused.load(Ordering::Relaxed);
                        self.send_to_panel(&PanelEvent::FilterPaused { paused });
                        let ns_enabled = self.noise_suppression.load(Ordering::Relaxed);
                        self.send_to_panel(&PanelEvent::NoiseSuppression { enabled: ns_enabled });
                    }
                }
            }

            // ---- State changes ----
            AppEvent::StateChanged(new_state) => {
                info!("State → {:?}", new_state);
                let js_str = new_state.as_js_str();
                let is_filtering = matches!(new_state, AppState::Filtering);
                let filter_active = is_filtering || matches!(new_state, AppState::ActiveStandby);

                // Update native tray menu status label
                if let Some(item) = &self.status_item {
                    let label = match &new_state {
                        AppState::ModelLoading    => "Voce — loading…",
                        AppState::OnboardingReady => "Voce — ready to enroll",
                        AppState::Recording { .. } => "Voce — recording…",
                        AppState::Adapting        => "Voce — adapting…",
                        AppState::TestReady       => "Voce — test your voice",
                        AppState::Testing         => "Voce — testing…",
                        AppState::PlayingBack
                        | AppState::PlayingBackRaw => "Voce — playing back…",
                        AppState::ActiveStandby
                        | AppState::Filtering     => "Voce — active ●",
                        AppState::Idle            => "Voce",
                    };
                    let _ = item.set_text(label);
                }

                {
                    let mut s = self.shared.lock().unwrap();
                    s.app_state = new_state;
                }
                self.send_to_panel(&PanelEvent::StateChanged { state: js_str });
                // Enable filter toggle tray item only while filter is running
                if let Some(item) = &self.filter_toggle_item {
                    let _ = item.set_enabled(filter_active);
                }

                if let Some(tray) = &self.tray_icon {
                    if is_filtering { let _ = tray.set_icon(Some(load_tray_icon_active())); }
                    else            { let _ = tray.set_icon(Some(load_tray_icon_idle())); }
                }
            }

            // ---- BlackHole status ----
            AppEvent::BlackHoleStatus { found } => {
                { self.shared.lock().unwrap().blackhole_found = found; }
                if found { info!("BlackHole 2ch detected"); }
                else     { warn!("BlackHole not found — install from https://existential.audio/blackhole/"); }
                self.send_to_panel(&PanelEvent::BlackholeStatus { found });
            }

            // ---- Download progress ----
            AppEvent::DownloadProgress { fraction } => {
                info!("Model download: {:.0}%", fraction * 100.0);
                self.send_to_panel(&PanelEvent::DownloadProgress { fraction });
            }

            // ---- Filter stats (Phase 5) ----
            AppEvent::FilterStats { similarity, passing } => {
                { self.shared.lock().unwrap().last_similarity = similarity; }
                self.send_to_panel(&PanelEvent::FilterStats { similarity, is_passing: passing });
                // Phase 7: update menu item text here
            }

            // ---- Recording events (Phase 4) ----
            AppEvent::RecordingProgress { index, elapsed_s, speech_s } => {
                self.send_to_panel(&PanelEvent::RecordingProgress { index, elapsed_s, speech_s });
            }
            AppEvent::RecordingComplete { index, speech_s } => {
                self.send_to_panel(&PanelEvent::RecordingComplete { index, speech_s });
            }
            AppEvent::RecordingInvalid { index } => {
                self.send_to_panel(&PanelEvent::RecordingInvalid { index, reason: "insufficient_speech" });
            }

            // ---- Tray icon click ----
            AppEvent::TrayIcon(tray_event) => {
                use tray_icon::TrayIconEvent;
                match tray_event {
                    TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        rect,
                        ..
                    } => {
                        if self.panel_open {
                            self.hide_panel();
                        } else {
                            self.ensure_panel(event_loop);
                            self.show_panel(&rect);
                        }
                    }
                    _ => {}
                }
            }

            // ---- Menu events ----
            AppEvent::Menu(menu_event) => {
                match menu_event.id().0.as_str() {
                    "quit" => {
                        info!("Quit");
                        event_loop.exit();
                    }
                    "reenroll" => {
                        info!("Re-enroll via menu");
                        let path = config::enrolled_embedding_path();
                        if path.exists() { let _ = std::fs::remove_file(&path); }
                        let _ = self.inference_cmd_tx.send(InferenceCmd::StopFilter);
                        let _ = self.proxy.send_event(AppEvent::StateChanged(AppState::OnboardingReady));
                        let _ = self.proxy.send_event(AppEvent::OpenPanel);
                    }
                    "toggle_filter" => {
                        let paused = !self.filter_paused.load(Ordering::Relaxed);
                        self.filter_paused.store(paused, Ordering::Relaxed);
                        let _ = self.proxy.send_event(AppEvent::FilterPaused { paused });
                    }
                    _ => {}
                }
            }

            // ---- Panel commands ----
            AppEvent::PanelCommand(cmd) => {
                info!("Panel command: {:?}", cmd);
                match cmd {
                    PanelCmd::OpenBlackholeLink => {
                        let _ = open::that("https://existential.audio/blackhole/");
                    }
                    PanelCmd::StartRecording { index } => {
                        let _ = self.inference_cmd_tx.send(InferenceCmd::StartEnrollment { index });
                    }
                    PanelCmd::StartTest => {
                        let _ = self.inference_cmd_tx.send(InferenceCmd::StartTest);
                    }
                    PanelCmd::StopTest => {
                        let _ = self.inference_cmd_tx.send(InferenceCmd::StopTest);
                    }
                    PanelCmd::ReplayTest => {
                        let _ = self.proxy.send_event(AppEvent::ReplayTest);
                    }
                    PanelCmd::ReplayTestRaw => {
                        let _ = self.proxy.send_event(AppEvent::ReplayTestRaw);
                    }
                    PanelCmd::StopPlayback => {
                        let _ = self.proxy.send_event(AppEvent::StopPlayback);
                    }
                    PanelCmd::ConfirmEnrollment => {
                        self.hide_panel();
                        let _ = self.proxy.send_event(AppEvent::StateChanged(AppState::ActiveStandby));
                    }
                    PanelCmd::Reenroll => {
                        let path = config::enrolled_embedding_path();
                        if path.exists() { let _ = std::fs::remove_file(&path); }
                        let _ = self.inference_cmd_tx.send(InferenceCmd::StopFilter);
                        let _ = self.proxy.send_event(AppEvent::StateChanged(AppState::OnboardingReady));
                    }
                    PanelCmd::ToggleFilter => {
                        let paused = !self.filter_paused.load(Ordering::Relaxed);
                        self.filter_paused.store(paused, Ordering::Relaxed);
                        let _ = self.proxy.send_event(AppEvent::FilterPaused { paused });
                    }
                    PanelCmd::SetNoiseSuppression { enabled } => {
                        self.noise_suppression.store(enabled, Ordering::Relaxed);
                        self.send_to_panel(&PanelEvent::NoiseSuppression { enabled });
                    }
                }
            }

            // ---- Enrolled profile ready (inference → main → back to inference) ----
            AppEvent::EnrolledProfileReady(arr) => {
                info!("Enrolled profile ready — activating filter");
                let _ = self.inference_cmd_tx.send(InferenceCmd::StartFilter { enrolled: arr });
            }

            // ---- Test capture events (Phase 6) ----
            AppEvent::TestProgress { elapsed_s } => {
                self.send_to_panel(&PanelEvent::TestProgress { elapsed_s });
            }
            AppEvent::TestCaptureComplete { samples, raw_samples, voice_pct } => {
                info!("Test capture complete — playing back {} samples", samples.len());
                self.last_test_samples = Some(samples.clone());
                self.last_test_raw_samples = Some(raw_samples.clone());
                self.send_to_panel(&PanelEvent::TestStats { voice_pct });
                self.playback_stop.store(false, Ordering::Relaxed);
                let proxy2 = self.proxy.clone();
                let stop_flag = self.playback_stop.clone();
                let _ = proxy2.send_event(AppEvent::StateChanged(AppState::PlayingBack));
                self.tokio.spawn(play_samples(samples, stop_flag, proxy2));
            }
            AppEvent::ReplayTest => {
                if let Some(samples) = self.last_test_samples.clone() {
                    info!("Replaying {} samples", samples.len());
                    self.playback_stop.store(false, Ordering::Relaxed);
                    let proxy2 = self.proxy.clone();
                    let stop_flag = self.playback_stop.clone();
                    let _ = proxy2.send_event(AppEvent::StateChanged(AppState::PlayingBack));
                    self.tokio.spawn(play_samples(samples, stop_flag, proxy2));
                } else {
                    warn!("ReplayTest requested but no samples stored");
                }
            }
            AppEvent::ReplayTestRaw => {
                if let Some(samples) = self.last_test_raw_samples.clone() {
                    info!("Replaying {} raw samples", samples.len());
                    self.playback_stop.store(false, Ordering::Relaxed);
                    let proxy2 = self.proxy.clone();
                    let stop_flag = self.playback_stop.clone();
                    let _ = proxy2.send_event(AppEvent::StateChanged(AppState::PlayingBackRaw));
                    self.tokio.spawn(play_samples(samples, stop_flag, proxy2));
                } else {
                    warn!("ReplayTestRaw requested but no raw samples stored");
                }
            }
            AppEvent::StopPlayback => {
                self.playback_stop.store(true, Ordering::Relaxed);
            }

            AppEvent::FilterPaused { paused } => {
                self.send_to_panel(&PanelEvent::FilterPaused { paused });
                if let Some(item) = &self.filter_toggle_item {
                    let label = if paused { "Resume filter" } else { "Pause filter" };
                    let _ = item.set_text(label);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}
}

// ---------------------------------------------------------------------------
// Audio playback helper
// ---------------------------------------------------------------------------

async fn play_samples(
    samples: Vec<f32>,
    stop: Arc<AtomicBool>,
    proxy: EventLoopProxy<AppEvent>,
) {
    use std::time::Duration;
    use std::thread;

    let stop_clone = stop.clone();
    let proxy_clone = proxy.clone();
    tokio::task::spawn_blocking(move || {
        use rodio::buffer::SamplesBuffer;
        use rodio::cpal::traits::{DeviceTrait, HostTrait};

        let host = rodio::cpal::default_host();
        let speaker = host.output_devices().ok().and_then(|mut devs| {
            devs.find(|d| {
                d.name()
                    .map(|n| {
                        let n = n.to_lowercase();
                        !n.contains("blackhole") && !n.contains("voce")
                    })
                    .unwrap_or(false)
            })
        });

        let stream_result = if let Some(device) = speaker {
            info!("Test playback via: {}", device.name().unwrap_or_default());
            rodio::OutputStreamBuilder::from_device(device)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .and_then(|b| b.open_stream().map_err(|e| anyhow::anyhow!("{e}")))
        } else {
            rodio::OutputStreamBuilder::open_default_stream()
                .map_err(|e| anyhow::anyhow!("{e}"))
        };

        match stream_result {
            Ok(stream) => {
                let sink = rodio::Sink::connect_new(stream.mixer());
                sink.append(SamplesBuffer::new(1u16, 16000u32, samples));
                loop {
                    if sink.empty() {
                        break;
                    }
                    if stop_clone.load(Ordering::Relaxed) {
                        sink.stop();
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                info!("Test playback complete");
            }
            Err(e) => warn!("Could not open audio output for playback: {e}"),
        }
        stop_clone.store(false, Ordering::Relaxed);
    }).await.ok();
    let _ = proxy.send_event(AppEvent::StateChanged(AppState::TestReady));
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

fn build_tray_menu() -> (muda::Menu, muda::MenuItem, muda::MenuItem) {
    use muda::{Menu, MenuItem, PredefinedMenuItem};
    let menu   = Menu::new();
    let status = MenuItem::with_id("status", "Voce — loading…", false, None);
    let filter_toggle = MenuItem::with_id("toggle_filter", "Pause filter", false, None);
    let _ = menu.append(&status);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&filter_toggle);
    let _ = menu.append(&MenuItem::with_id("reenroll", "Re-enroll voice…", true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("quit", "Quit Voce", true, None));
    (menu, status, filter_toggle)
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

fn load_tray_icon_loading() -> tray_icon::Icon {
    load_icon_from_png(include_bytes!("../assets/icons/voce_loading.png"))
}
fn load_tray_icon_idle() -> tray_icon::Icon {
    load_icon_from_png(include_bytes!("../assets/icons/voce_idle.png"))
}
fn load_tray_icon_active() -> tray_icon::Icon {
    load_icon_from_png(include_bytes!("../assets/icons/voce_active.png"))
}

fn load_icon_from_png(png_bytes: &[u8]) -> tray_icon::Icon {
    let img = image::load_from_memory(png_bytes)
        .expect("invalid icon PNG")
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("invalid icon dimensions")
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "Voce")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Args {
    #[arg(long)]
    eval_enroll: Option<PathBuf>,

    #[arg(long)]
    eval_enroll_2: Option<PathBuf>,

    #[arg(long)]
    eval_enroll_out: Option<PathBuf>,

    #[arg(long)]
    eval: Option<PathBuf>,

    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long)]
    enrollment: Option<PathBuf>,
}

fn init_eval_logging() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("voce=warn")),
        )
        .init();
}

fn build_rt() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

fn main() -> anyhow::Result<()> {
    // --- Early intercepts: eval modes bypass all GUI initialisation ---
    let args = Args::parse();

    // --eval-enroll <wav> [--eval-enroll-2 <wav>] [--eval-enroll-out <path>]
    if let Some(wav) = args.eval_enroll {
        init_eval_logging();
        let rt = build_rt()?;
        let code = rt.block_on(eval::run_enroll_from_wav(wav, args.eval_enroll_2, args.eval_enroll_out))?;
        std::process::exit(code);
    }

    // --eval <wav> [--output <path>] [--enrollment <path>]
    if let Some(wav) = args.eval {
        init_eval_logging();
        let rt = build_rt()?;
        let code = rt.block_on(eval::run_eval(wav, args.output, args.enrollment))?;
        std::process::exit(code);
    }

    // Install bundled VoceAudio HAL driver if not already present.
    // Non-fatal: if it fails the user falls back to BlackHole.
    if let Err(e) = driver::ensure_installed() {
        tracing::warn!("VoceAudio driver install skipped: {e}");
    }

    let voce_dir = config::voce_dir();
    std::fs::create_dir_all(&voce_dir)?;

    let log_writer = tracing_appender::rolling::daily(&voce_dir, "voce.log");

    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("voce=debug".parse().unwrap());

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(log_writer))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();

    info!("Voce v{} — logging to: {}", env!("CARGO_PKG_VERSION"), voce_dir.display());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2).enable_all().build()?;
    let tokio_handle = rt.handle().clone();
    let _rt = rt;

    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut app = VoceApp::new(proxy, tokio_handle);
    event_loop.run_app(&mut app)?;
    Ok(())
}
