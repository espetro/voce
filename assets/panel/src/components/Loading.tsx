import type { Component } from 'solid-js';
import { Show } from 'solid-js';
import { AlertTriangle } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';

interface Props { state: AppState; ipc: IpcActions; }

const Loading: Component<Props> = (props) => {
  const label = () => props.state.isDownloading()
    ? `Downloading model… ${Math.round(props.state.downloadFraction() * 100)}%`
    : 'Loading voice model…';

  return (
    <div>
      <div class="status-row">
        <div class="dot dot-orange pulse" />
        <span>{label()}</span>
      </div>
      <div class="progress-wrap">
        <div
          class={props.state.isDownloading() ? 'progress-bar' : 'progress-bar progress-bar-indeterminate'}
          style={props.state.isDownloading() ? { width: `${props.state.downloadFraction() * 100}%` } : undefined}
        />
      </div>
      <p class="body-text">
        Voce learns to recognise your voice and filters out everyone else's on your calls.
      </p>
      <Show when={!props.state.blackholeFound()}>
        <div class="warning-banner">
          <AlertTriangle size={14} />
          <span>
            Voce Microphone driver not found. Run{' '}
            <a onclick={props.ipc.openBlackholeLink}>make -C audio-driver reload</a>.
          </span>
        </div>
      </Show>
    </div>
  );
};

export default Loading;
