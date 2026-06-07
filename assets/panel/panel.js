'use strict';

// ---- IPC bridge ----

/** Send a command object to Rust via wry IPC. */
function send(obj) {
  if (window.ipc) {
    window.ipc.postMessage(JSON.stringify(obj));
  } else {
    console.warn('[voce] ipc not available', obj);
  }
}

// ---- State machine ----

let currentScreen = null;
let blackholeFound = true;
let hasTestedOnce = false; // true after first test completes

/** Show only the named screen, hide all others. */
function showScreen(id) {
  if (currentScreen === id) return;
  currentScreen = id;
  document.querySelectorAll('.screen').forEach(el => el.classList.remove('active'));
  const el = document.getElementById(id);
  if (el) {
    el.classList.add('active');
  } else {
    console.warn('[voce] unknown screen:', id);
  }
}

/** Show/hide the BlackHole warning inside a given screen element. */
function setBlackholeWarning(found) {
  blackholeFound = found;
  ['blackhole-warning', 'blackhole-warning-ob'].forEach(id => {
    const el = document.getElementById(id);
    if (el) el.style.display = found ? 'none' : 'flex';
  });
}

// ---- Event handlers ----

/** Rust → JS entry point: called by evaluate_script("window.__voce_update(...)") */
window.__voce_update = function(jsonStr) {
  let msg;
  try {
    msg = JSON.parse(jsonStr);
  } catch(e) {
    console.error('[voce] JSON parse error:', e, jsonStr);
    return;
  }

  switch (msg.event) {

    case 'state_changed':
      handleStateChanged(msg.state);
      break;

    case 'recording_progress': {
      const { index, elapsed_s, speech_s: _s } = msg;
      const elapsed = elapsed_s || 0;
      const remaining = Math.max(0, 20 - elapsed);
      const pct = Math.min(100, (elapsed / 20) * 100);
      if (index === 1) {
        setText('rec1-remaining', remaining);
        setWidth('rec1-bar', pct);
      } else {
        setText('rec2-remaining', remaining);
        setWidth('rec2-bar', pct);
      }
      break;
    }

    case 'recording_complete':
      if (msg.index === 1) {
        showScreen('screen-recording2-ready');
      }
      // index 2 completion → ADAPTING state will follow via state_changed
      break;

    case 'recording_invalid':
      showScreen(msg.index === 1 ? 'screen-rec1-invalid' : 'screen-rec2-invalid');
      break;

    case 'test_progress': {
      const elapsed = msg.elapsed_s || 0;
      const remaining = Math.max(0, 10 - elapsed);
      const pct = Math.min(100, (elapsed / 10) * 100);
      setText('test-remaining', remaining);
      setWidth('test-bar', pct);
      break;
    }

    case 'filter_stats': {
      const sim = typeof msg.similarity === 'number' ? msg.similarity.toFixed(2) : '—';
      setText('similarity-val', sim);
      const dot = document.getElementById('active-dot');
      if (dot) {
        dot.classList.toggle('pulse', msg.is_passing === true);
      }
      break;
    }

    case 'blackhole_status':
      setBlackholeWarning(msg.found === true);
      break;

    case 'download_progress': {
      const pct = Math.round((msg.fraction || 0) * 100);
      const bar = document.getElementById('loading-bar');
      if (bar) {
        bar.classList.remove('indeterminate');
        bar.style.width = pct + '%';
      }
      setText('loading-label', 'Downloading model… ' + pct + '%');
      break;
    }

    default:
      console.log('[voce] unhandled event:', msg);
  }
};

function handleStateChanged(state) {
  switch (state) {
    case 'IDLE':
    case 'MODEL_LOADING':
      showScreen('screen-loading');
      break;
    case 'ONBOARDING_READY':
      hasTestedOnce = false;
      showScreen('screen-onboarding');
      break;
    case 'RECORDING_1':
      showScreen('screen-recording1');
      break;
    case 'RECORDING_2':
      showScreen('screen-recording2');
      break;
    case 'ADAPTING':
      showScreen('screen-adapting');
      break;
    case 'TEST_READY':
      showScreen(hasTestedOnce ? 'screen-test-complete' : 'screen-test-ready');
      break;
    case 'TESTING':
      hasTestedOnce = true;
      showScreen('screen-testing');
      break;
    case 'PLAYING_BACK':
      showScreen('screen-playback');
      break;
    case 'ACTIVE_STANDBY':
    case 'FILTERING':
      showScreen('screen-active');
      setText('active-label', 'Active — Voce Microphone');
      break;
    default:
      console.warn('[voce] unknown state:', state);
  }
}

// ---- DOM helpers ----

function setText(id, val) {
  const el = document.getElementById(id);
  if (el) el.textContent = val;
}

function setWidth(id, pct) {
  const el = document.getElementById(id);
  if (el) el.style.width = pct + '%';
}

// ---- Init ----
// Start in loading screen; Rust will send state_changed immediately
showScreen('screen-loading');
