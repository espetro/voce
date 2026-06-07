import type { Component } from 'solid-js';
import { Settings as SettingsIcon } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';
import ToggleSwitch from './ToggleSwitch';

interface Props {
  state: AppState;
  ipc: IpcActions;
  onOpenSettings: () => void;
}

const Active: Component<Props> = (props) => {
  const simText = () => {
    const s = props.state.similarity();
    return s !== null ? s.toFixed(2) : '—';
  };

  const dotClass = () => {
    if (props.state.filterPaused()) return 'dot dot-grey';
    return props.state.isPassing() ? 'dot dot-green pulse' : 'dot dot-green';
  };

  return (
    <div>
      <div class="header-row">
        <div class="header-status">
          <div class={dotClass()} />
          <span>
            {props.state.filterPaused()
              ? 'Filter paused — all audio passing'
              : 'Active — Voce Microphone'}
          </span>
        </div>
        <button class="icon-btn" onclick={props.onOpenSettings} aria-label="Settings">
          <SettingsIcon size={16} />
        </button>
      </div>

      <p class="body-text">
        In your call app, set your microphone input to <strong>Voce Microphone</strong>.
      </p>

      <div class="section-title">Filter</div>
      <ToggleSwitch
        checked={!props.state.filterPaused()}
        onChange={() => props.ipc.toggleFilter()}
        label={props.state.filterPaused() ? 'Off' : 'On'}
      />

      <div class="stats-row">
        <span>Similarity (last 3 s)</span>
        <span class="stats-value">{simText()}</span>
      </div>

      <hr class="divider" />
      <button class="btn btn-secondary" onclick={props.ipc.startTest}>Test voice filter</button>
      <button class="btn btn-secondary" onclick={props.ipc.reenroll}>Re-enroll voice…</button>
    </div>
  );
};

export default Active;
