import type { Component } from 'solid-js';
import { Show, onMount } from 'solid-js';
import { CheckCircle } from 'lucide-solid';
import type { AppState } from '../hooks/useAppState';
import type { IpcActions } from '../hooks/useIpc';

interface Props {
  state: AppState;
  ipc: IpcActions;
}

const DriverSetup: Component<Props> = (props) => {
  const driverInstalled = () => props.state.driverInstalled();
  const deviceFound = () => props.state.voceDeviceFound();

  onMount(() => {
    if (!driverInstalled()) {
      props.ipc.installDriver();
    }
  });

  const handleContinue = () => {
    props.state.setHasSeenDriverSetup(true);
    props.state.setScreen('active');
  };

  return (
    <div class="flex flex-col items-center justify-center min-h-screen bg-gradient-to-b from-slate-900 to-slate-950 text-white p-6">
      <Show
        when={!driverInstalled()}
        fallback={
          <Show
            when={deviceFound()}
            fallback={
              <div class="text-center space-y-6 max-w-md">
                <h1 class="text-2xl font-bold">One more step</h1>
                <p class="text-slate-300">
                  In Zoom, Meet, or Teams, set the microphone to Voce Microphone.
                </p>
                <button
                  onclick={() => props.ipc.openSystemSound()}
                  class="w-full px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition"
                >
                  Open Sound Settings
                </button>
                <button
                  onclick={handleContinue}
                  class="w-full text-slate-400 hover:text-slate-300 text-sm font-medium transition"
                >
                  Already set up
                </button>
              </div>
            }
          >
            <div class="text-center space-y-6 max-w-md">
              <div class="flex justify-center">
                <CheckCircle size={48} class="text-green-500" />
              </div>
              <h1 class="text-2xl font-bold">Voce Microphone is ready</h1>
              <p class="text-slate-300">You're all set! Use Voce Microphone in your call app.</p>
              <button
                onclick={handleContinue}
                class="w-full px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition"
              >
                Continue →
              </button>
            </div>
          </Show>
        }
      >
        <div class="text-center space-y-6 max-w-md">
          <div class="flex justify-center">
            <div class="relative w-12 h-12">
              <div class="absolute inset-0 bg-blue-500 rounded-full animate-pulse"></div>
              <div class="absolute inset-2 bg-slate-950 rounded-full"></div>
            </div>
          </div>
          <h1 class="text-2xl font-bold">Setting up Voce Microphone…</h1>
          <div class="w-full h-1 bg-slate-700 rounded overflow-hidden">
            <div class="h-full bg-blue-500 animate-pulse"></div>
          </div>
          <button
            onclick={handleContinue}
            class="text-slate-400 hover:text-slate-300 text-sm font-medium transition"
          >
            Already set up
          </button>
        </div>
      </Show>
    </div>
  );
};

export default DriverSetup;
