import type { Component } from 'solid-js';
import { Switch, Match, Show } from 'solid-js';
import { AlertTriangle, CheckCircle } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';
import Footer from './Footer';

interface Props { state: AppState; ipc: IpcActions; }

const Onboarding: Component<Props> = (props) => {
  const s = () => props.state.screen();
  const rec1Pct = () => Math.min(100, (props.state.rec1Elapsed() / 20) * 100);
  const rec2Pct = () => Math.min(100, (props.state.rec2Elapsed() / 20) * 100);

  return (
    <>
      <Switch>
        {/* Screen 02 — Onboarding ready */}
        <Match when={s() === 'onboarding-ready'}>
          <div class="status-row">
            <div class="dot dot-green" />
            <span>Model ready</span>
          </div>
          <Show when={!props.state.blackholeFound()}>
            <div class="warning-banner">
              <AlertTriangle size={14} />
              <span>
                Voce Microphone driver not found. Run{' '}
                <a onclick={props.ipc.openBlackholeLink}>make -C audio-driver reload</a>.
              </span>
            </div>
          </Show>
          <div class="section-title">Step 1 of 3 — Record Your Voice</div>
          <p class="body-text">
            Speak naturally for 20 seconds. Read anything aloud — an article, your emails, count
            numbers. The app needs to hear you clearly.
          </p>
          <div class="badge">Recording 1 of 2</div>
          <button class="btn btn-primary" onclick={() => props.ipc.startRecording(1)}>
            Start Recording
          </button>
        </Match>

        {/* Screen 03 — Recording 1 in progress */}
        <Match when={s() === 'recording-1'}>
          <div class="status-row">
            <div class="dot dot-orange pulse" />
            <span>Recording… {Math.max(0, 20 - props.state.rec1Elapsed())}s remaining</span>
          </div>
          <div class="progress-wrap">
            <div class="progress-bar" style={{ width: `${rec1Pct()}%` }} />
          </div>
          <p class="body-text">Keep talking naturally.</p>
          <div class="badge">Recording 1 of 2</div>
        </Match>

        {/* Screen 04 — Recording 1 invalid */}
        <Match when={s() === 'recording-1-invalid'}>
          <div class="status-row">
            <div class="dot dot-grey" />
            <span>Not enough speech detected</span>
          </div>
          <p class="body-text">Please try again in a quieter environment.</p>
          <button class="btn btn-primary" onclick={() => props.ipc.startRecording(1)}>
            Retry
          </button>
        </Match>

        {/* Screen 05 — Recording 1 complete */}
        <Match when={s() === 'recording-1-complete'}>
          <div class="status-row">
            <div class="dot dot-green" />
            <span style={{ display: 'flex', 'align-items': 'center', gap: '4px' }}>
              <CheckCircle size={13} />
              Recording 1 complete
            </span>
          </div>
          <div class="section-title">Step 2 of 3 — One More Recording</div>
          <p class="body-text">Same thing — speak naturally for 20 seconds.</p>
          <div class="badge">Recording 2 of 2</div>
          <button class="btn btn-primary" onclick={() => props.ipc.startRecording(2)}>
            Start Recording
          </button>
        </Match>

        {/* Screen 06 — Recording 2 in progress */}
        <Match when={s() === 'recording-2'}>
          <div class="status-row">
            <div class="dot dot-orange pulse" />
            <span>Recording… {Math.max(0, 20 - props.state.rec2Elapsed())}s remaining</span>
          </div>
          <div class="progress-wrap">
            <div class="progress-bar" style={{ width: `${rec2Pct()}%` }} />
          </div>
          <p class="body-text">Keep talking naturally.</p>
          <div class="badge">Recording 2 of 2</div>
        </Match>

        {/* Screen 07 — Recording 2 invalid */}
        <Match when={s() === 'recording-2-invalid'}>
          <div class="status-row">
            <div class="dot dot-grey" />
            <span>Not enough speech detected</span>
          </div>
          <p class="body-text">Please try again in a quieter environment.</p>
          <button class="btn btn-primary" onclick={() => props.ipc.startRecording(2)}>
            Retry
          </button>
        </Match>

        {/* Screen 08 — Adapting (no footer) */}
        <Match when={s() === 'adapting'}>
          <div class="status-row">
            <div class="dot dot-orange pulse" />
            <span>Adapting to your voice…</span>
          </div>
          <div class="progress-wrap">
            <div class="progress-bar progress-bar-indeterminate" />
          </div>
          <p class="body-text">Voce is building your voice profile. This takes just a moment.</p>
        </Match>
      </Switch>
      <Footer screen={s()} />
    </>
  );
};

export default Onboarding;
