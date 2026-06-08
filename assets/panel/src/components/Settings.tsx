import type { Component } from 'solid-js';
import { createSignal, Show } from 'solid-js';
import { ArrowLeft, CheckCircle, Circle } from 'lucide-solid';
import { Dialog } from '@ark-ui/solid/dialog';
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

const Settings: Component<Props> = (props) => {
  const [showResetDialog, setShowResetDialog] = createSignal(false);
  const [showStatusTooltip, setShowStatusTooltip] = createSignal(false);

  const handleReset = () => {
    props.ipc.fullReset();
    setShowResetDialog(false);
  };

  return (
    <div class="flex flex-col h-screen bg-slate-950">
      <div class="settings-header">
        <button class="settings-back-btn" onclick={props.onBack} aria-label="Back">
          <ArrowLeft size={16} />
        </button>
        <span>Settings</span>
      </div>
      <hr class="divider" />

      <div class="flex-1 overflow-y-auto">
        <div class="section-title">AUDIO</div>
        <div class="section-title">Noise Suppression</div>
        <ToggleSwitch
          checked={props.state.noiseSuppression()}
          onChange={(enabled) => props.ipc.setNoiseSuppression(enabled)}
          label={props.state.noiseSuppression() ? 'On' : 'Off'}
        />
        <p class="body-text">Reduces background noise…</p>

        <div class="section-title">Voice Filter</div>
        <ToggleSwitch
          checked={!props.state.filterPaused()}
          onChange={() => props.ipc.toggleFilter()}
          label={props.state.filterPaused() ? 'Off' : 'On'}
        />
        <p class="body-text">Filters other speakers…</p>

        <hr class="divider" />

        <div class="section-title">VOCE MICROPHONE</div>
        <div class="flex items-center gap-3 px-4 py-3 rounded-lg bg-slate-900/50">
          <Show
            when={props.state.voceDeviceFound()}
            fallback={<Circle size={20} class="text-slate-400" />}
          >
            <CheckCircle size={20} class="text-green-500" />
          </Show>
          <span class="text-sm font-medium text-slate-200">
            {props.state.voceDeviceFound() ? 'Device active' : 'Device not found'}
          </span>
        </div>

        <hr class="divider" />

        <div class="section-title">ENROLLMENT</div>
        <button class="btn btn-secondary w-full" onclick={() => props.ipc.reenroll()}>
          Re-enroll voice…
        </button>

        <hr class="divider" />

        <div class="section-title">DATA</div>
        <button
          class="btn btn-danger w-full"
          onclick={() => setShowResetDialog(true)}
        >
          Reset everything…
        </button>
      </div>

      <hr class="divider" />

      <div class="relative bg-slate-900/50 px-4 py-3 flex items-center justify-between">
        <div class="flex items-center gap-3">
          <img
            src={props.state.filterPaused() ? logoOff : logoOn}
            alt="Voce"
            class="w-6 h-6"
          />
          <span class="text-sm font-medium text-slate-200">● Voce Microphone</span>
        </div>
        <button
          class="text-slate-400 hover:text-slate-300 transition"
          onclick={() => setShowStatusTooltip(!showStatusTooltip())}
          aria-label="Toggle status tooltip"
        >
          ▾
        </button>

        <Show when={showStatusTooltip()}>
          <div class="absolute bottom-full left-4 right-4 mb-2 bg-slate-800 border border-slate-700 rounded-lg p-3 text-xs space-y-2 z-10 shadow-lg">
            <div class="flex items-center gap-2 text-slate-300">
              <Show
                when={props.state.driverInstalled()}
                fallback={<Circle size={12} class="text-slate-500" />}
              >
                <Circle size={12} class="text-green-500" />
              </Show>
              <span>Driver {props.state.driverInstalled() ? 'installed' : 'not installed'}</span>
            </div>
            <div class="flex items-center gap-2 text-slate-300">
              <Show
                when={props.state.voceDeviceFound()}
                fallback={<Circle size={12} class="text-slate-500" />}
              >
                <Circle size={12} class="text-green-500" />
              </Show>
              <span>Device {props.state.voceDeviceFound() ? 'active' : 'not found'}</span>
            </div>
          </div>
        </Show>
      </div>

      <Dialog.Root open={showResetDialog()} onOpenChange={(open) => setShowResetDialog(open)}>
        <Dialog.Backdrop class="fixed inset-0 bg-black/50" />
        <Dialog.Content class="fixed inset-0 flex items-center justify-center z-50">
          <div class="bg-slate-900 rounded-lg p-6 max-w-sm w-full mx-4 space-y-4">
            <Dialog.Title class="text-lg font-bold text-white">Reset Voce?</Dialog.Title>
            <Dialog.Description class="text-sm text-slate-400 space-y-2">
              <p>This will permanently remove:</p>
              <ul class="list-disc list-inside text-slate-400">
                <li>Your voice enrollment profile</li>
                <li>App configuration</li>
                <li>Downloaded models (re-downloaded on next launch)</li>
                <li>Voce Microphone audio driver</li>
              </ul>
            </Dialog.Description>
            <div class="flex gap-3 pt-4">
              <button
                class="flex-1 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition"
                onclick={() => setShowResetDialog(false)}
              >
                Cancel
              </button>
              <button
                class="flex-1 px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg transition"
                onclick={handleReset}
              >
                Reset
              </button>
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Root>
    </div>
  );
};

export default Settings;
