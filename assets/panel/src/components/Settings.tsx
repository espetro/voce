import type { Component } from 'solid-js';
import { ArrowLeft } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';
import ToggleSwitch from './ToggleSwitch';
import logoOff from '../assets/logo-off.svg';
import logoOn from '../assets/logo-on.svg';

interface Props {
  state: AppState;
  ipc: IpcActions;
  onBack: () => void;
}

const Settings: Component<Props> = (props) => (
  <div>
    <div class="settings-header">
      <button class="settings-back-btn" onclick={props.onBack} aria-label="Back">
        <ArrowLeft size={16} />
      </button>
      <span>Settings</span>
    </div>
    <hr class="divider" />

    <div class="section-title">Noise Suppression</div>
    <ToggleSwitch
      checked={props.state.noiseSuppression()}
      onChange={(enabled) => props.ipc.setNoiseSuppression(enabled)}
      label={props.state.noiseSuppression() ? 'On' : 'Off'}
    />
    <p class="body-text">Reduces background noise before speaker filtering.</p>

    <hr class="divider" />

    <div class="section-title">Voice Filter</div>
    <ToggleSwitch
      checked={!props.state.filterPaused()}
      onChange={() => props.ipc.toggleFilter()}
      label={props.state.filterPaused() ? 'Off' : 'On'}
    />
    <p class="body-text">Filters out other speakers in real time.</p>

    <hr class="divider" />
    <button class="btn btn-secondary" onclick={props.ipc.reenroll}>Re-enroll voice…</button>

    <div class="settings-footer">
      <img
        src={props.state.filterPaused() ? logoOff : logoOn}
        alt="Voce"
        class="settings-logo"
      />
    </div>
  </div>
);

export default Settings;
