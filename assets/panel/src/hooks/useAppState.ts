import { createSignal } from 'solid-js';
import type { Screen } from '../types';

type VoceMsg = Record<string, unknown>;
type VoceWindow = Window & { __voce_update: (s: string) => void };

export function createAppState() {
  const [screen, setScreen] = createSignal<Screen>('loading');
  const [blackholeFound, setBlackholeFound] = createSignal(true);
  const [hasTestedOnce, setHasTestedOnce] = createSignal(false);

  const [rec1Elapsed, setRec1Elapsed] = createSignal(0);
  const [rec2Elapsed, setRec2Elapsed] = createSignal(0);
  const [testElapsed, setTestElapsed] = createSignal(0);

  const [filterPaused, setFilterPaused] = createSignal(false);
  const [similarity, setSimilarity] = createSignal<number | null>(null);
  const [isPassing, setIsPassing] = createSignal(false);

  const [voicePct, setVoicePct] = createSignal(0);
  const [noiseSuppression, setNoiseSuppression] = createSignal(true);

  const [downloadFraction, setDownloadFraction] = createSignal(0);
  const [isDownloading, setIsDownloading] = createSignal(false);

  function handleStateChanged(state: string) {
    switch (state) {
      case 'IDLE':
      case 'MODEL_LOADING':
        setScreen('loading');
        break;
      case 'ONBOARDING_READY':
        setHasTestedOnce(false);
        setFilterPaused(false);
        setScreen('onboarding-ready');
        break;
      case 'RECORDING_1':
        setRec1Elapsed(0);
        setFilterPaused(false);
        setScreen('recording-1');
        break;
      case 'RECORDING_2':
        setRec2Elapsed(0);
        setFilterPaused(false);
        setScreen('recording-2');
        break;
      case 'ADAPTING':
        setFilterPaused(false);
        setScreen('adapting');
        break;
      case 'TEST_READY':
        setFilterPaused(false);
        setScreen(hasTestedOnce() ? 'test-complete' : 'test-ready');
        break;
      case 'TESTING':
        setHasTestedOnce(true);
        setTestElapsed(0);
        setFilterPaused(false);
        setScreen('testing');
        break;
      case 'PLAYING_BACK':
        setScreen('playback-filtered');
        break;
      case 'PLAYING_BACK_RAW':
        setScreen('playback-raw');
        break;
      case 'ACTIVE_STANDBY':
      case 'FILTERING':
        setFilterPaused(false);
        setSimilarity(null);
        setScreen('active');
        break;
    }
  }

  (window as unknown as VoceWindow).__voce_update = (jsonStr: string) => {
    let msg: VoceMsg;
    try { msg = JSON.parse(jsonStr); } catch { return; }

    switch (msg.event) {
      case 'state_changed':
        handleStateChanged(msg.state as string);
        break;
      case 'recording_progress': {
        const elapsed = (msg.elapsed_s as number) ?? 0;
        if (msg.index === 1) setRec1Elapsed(elapsed);
        else setRec2Elapsed(elapsed);
        break;
      }
      case 'recording_complete':
        if (msg.index === 1) setScreen('recording-1-complete');
        break;
      case 'recording_invalid':
        setScreen(msg.index === 1 ? 'recording-1-invalid' : 'recording-2-invalid');
        break;
      case 'test_progress':
        setTestElapsed((msg.elapsed_s as number) ?? 0);
        break;
      case 'filter_stats':
        setSimilarity(typeof msg.similarity === 'number' ? msg.similarity : null);
        setIsPassing(msg.is_passing === true);
        break;
      case 'blackhole_status':
        setBlackholeFound(msg.found === true);
        break;
      case 'download_progress':
        setIsDownloading(true);
        setDownloadFraction((msg.fraction as number) ?? 0);
        break;
      case 'test_stats':
        setVoicePct((msg.voice_pct as number) ?? 0);
        break;
      case 'filter_paused':
        setFilterPaused(msg.paused === true);
        if (msg.paused) setSimilarity(null);
        break;
      case 'noise_suppression':
        setNoiseSuppression(msg.enabled !== false);
        break;
    }
  };

  return {
    screen, setScreen,
    blackholeFound,
    rec1Elapsed, rec2Elapsed, testElapsed,
    filterPaused, similarity, isPassing,
    voicePct, noiseSuppression,
    downloadFraction, isDownloading,
  };
}

export type AppState = ReturnType<typeof createAppState>;
