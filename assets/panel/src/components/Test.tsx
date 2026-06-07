import type { Component } from 'solid-js';
import { Switch, Match } from 'solid-js';
import { CheckCircle, AlertCircle, XCircle } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';
import Footer from './Footer';

interface Props { state: AppState; ipc: IpcActions; }

const Test: Component<Props> = (props) => {
  const s = () => props.state.screen();
  const pct = () => props.state.voicePct();
  const testPct = () => Math.min(100, (props.state.testElapsed() / 10) * 100);
  const tier = () => pct() >= 60 ? 'good' : pct() >= 40 ? 'marginal' : 'poor';

  return (
    <>
      <Switch>
        {/* Screen 09 — Test ready */}
        <Match when={s() === 'test-ready'}>
          <div class="status-row">
            <div class="dot dot-green" />
            <span>Ready — your voice profile is saved</span>
          </div>
          <hr class="divider" />
          <div class="section-title">Test it out</div>
          <p class="body-text">
            Click Record, speak for a few seconds (try having someone else talk too), then click
            Stop. You'll hear the filtered playback — only your voice should come through.
          </p>
          <button class="btn btn-primary" onclick={props.ipc.startTest}>
            Start Recording
          </button>
        </Match>

        {/* Screen 10 — Testing in progress */}
        <Match when={s() === 'testing'}>
          <div class="status-row">
            <div class="dot dot-orange pulse" />
            <span>Recording — {Math.max(0, 10 - props.state.testElapsed())}s remaining</span>
          </div>
          <div class="progress-wrap">
            <div class="progress-bar" style={{ width: `${testPct()}%` }} />
          </div>
          <p class="body-text">Speak naturally. Voce will stop automatically.</p>
          <button class="btn btn-danger" onclick={props.ipc.stopTest}>Stop early</button>
        </Match>

        {/* Screen 11 — Playback filtered */}
        <Match when={s() === 'playback-filtered'}>
          <div class="status-row">
            <div class="dot dot-green pulse" />
            <span>Playing your filtered voice…</span>
          </div>
          <p class="body-text">Listen — only your voice should come through.</p>
          <button class="btn btn-secondary" onclick={props.ipc.stopPlayback}>Stop</button>
        </Match>

        {/* Screen 12 — Playback raw */}
        <Match when={s() === 'playback-raw'}>
          <div class="status-row">
            <div class="dot dot-green pulse" />
            <span>Playing original mic audio…</span>
          </div>
          <p class="body-text">This is the unfiltered audio — including all voices.</p>
          <button class="btn btn-secondary" onclick={props.ipc.stopPlayback}>Stop</button>
        </Match>

        {/* Screen 13 — Test complete (tiered quality UI) */}
        <Match when={s() === 'test-complete'}>
          <div class="status-row">
            <div class={tier() === 'poor' ? 'dot dot-grey' : 'dot dot-green'} />
            <span>How did it sound?</span>
          </div>

          <Switch>
            <Match when={tier() === 'good'}>
              <div class="quality-banner quality-banner-green">
                <div class="quality-label quality-label-green">
                  <CheckCircle size={14} />
                  {pct() >= 80 ? 'Crystal clear' : 'Crisp and defined'}
                </div>
              </div>
              <button class="btn btn-primary" onclick={props.ipc.confirmEnrollment}>
                Looks good — I'm done
              </button>
              <button class="btn btn-secondary" onclick={props.ipc.replayTest}>Replay filtered</button>
              <button class="btn btn-secondary" onclick={props.ipc.replayTestRaw}>Play original</button>
              <button class="btn btn-secondary" onclick={props.ipc.startTest}>Re-test</button>
            </Match>

            <Match when={tier() === 'marginal'}>
              <div class="quality-banner quality-banner-amber">
                <div class="quality-label quality-label-amber">
                  <AlertCircle size={14} />
                  Coming through
                </div>
                <div class="quality-hint">Consider re-testing in a quieter room.</div>
              </div>
              <button class="btn btn-secondary" onclick={props.ipc.confirmEnrollment}>
                Looks good — I'm done
              </button>
              <button class="btn btn-secondary" onclick={props.ipc.replayTest}>Replay filtered</button>
              <button class="btn btn-secondary" onclick={props.ipc.replayTestRaw}>Play original</button>
              <button class="btn btn-secondary" onclick={props.ipc.startTest}>Re-test</button>
            </Match>

            <Match when={tier() === 'poor'}>
              <div class="quality-banner quality-banner-red">
                <div class="quality-label quality-label-red">
                  <XCircle size={14} />
                  Low match detected
                </div>
                <div class="quality-hint">Try re-enrolling or speak closer to the mic.</div>
              </div>
              <button class="btn btn-primary" onclick={props.ipc.reenroll}>Re-enroll voice…</button>
              <button class="btn btn-secondary" onclick={props.ipc.startTest}>Re-test</button>
            </Match>
          </Switch>
        </Match>
      </Switch>
      <Footer screen={s()} />
    </>
  );
};

export default Test;
