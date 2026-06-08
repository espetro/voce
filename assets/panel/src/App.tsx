import type { Component } from 'solid-js';
import { Switch, Match } from 'solid-js';
import { createAppState } from './hooks/useAppState';
import { useIpc } from './hooks/useIpc';
import Loading from './components/Loading';
import DriverSetup from './components/DriverSetup';
import Onboarding from './components/Onboarding';
import Test from './components/Test';
import Active from './components/Active';
import Settings from './components/Settings';
import type { Screen } from './types';

const ONBOARDING_SCREENS: Screen[] = [
  'onboarding-ready', 'recording-1', 'recording-1-invalid',
  'recording-1-complete', 'recording-2', 'recording-2-invalid', 'adapting',
];

const TEST_SCREENS: Screen[] = [
  'test-ready', 'testing', 'playback-filtered', 'playback-raw', 'test-complete',
];

const App: Component = () => {
  const state = createAppState();
  const ipc = useIpc();

  return (
    <Switch fallback={<Loading state={state} ipc={ipc} />}>
      <Match when={state.screen() === 'loading'}>
        <Loading state={state} ipc={ipc} />
      </Match>
      <Match when={state.screen() === 'driver-setup'}>
        <DriverSetup state={state} ipc={ipc} />
      </Match>
      <Match when={ONBOARDING_SCREENS.includes(state.screen())}>
        <Onboarding state={state} ipc={ipc} />
      </Match>
      <Match when={TEST_SCREENS.includes(state.screen())}>
        <Test state={state} ipc={ipc} />
      </Match>
      <Match when={state.screen() === 'active'}>
        <Active
          state={state}
          ipc={ipc}
          onOpenSettings={() => state.setScreen('settings')}
        />
      </Match>
      <Match when={state.screen() === 'settings'}>
        <Settings state={state} ipc={ipc} onBack={() => state.setScreen('active')} />
      </Match>
    </Switch>
  );
};

export default App;
