type WryWindow = Window & { ipc?: { postMessage: (s: string) => void } };

function send(cmd: Record<string, unknown>): void {
  const ipc = (window as WryWindow).ipc;
  if (ipc) ipc.postMessage(JSON.stringify(cmd));
  else console.warn('[voce] ipc not available', cmd);
}

export const useIpc = () => ({
  startRecording: (index: number) => send({ cmd: 'start_recording', index }),
  startTest: () => send({ cmd: 'start_test' }),
  stopTest: () => send({ cmd: 'stop_test' }),
  replayTest: () => send({ cmd: 'replay_test' }),
  replayTestRaw: () => send({ cmd: 'replay_test_raw' }),
  stopPlayback: () => send({ cmd: 'stop_playback' }),
  confirmEnrollment: () => send({ cmd: 'confirm_enrollment' }),
  reenroll: () => send({ cmd: 'reenroll' }),
  openBlackholeLink: () => send({ cmd: 'open_blackhole_link' }),
  toggleFilter: () => send({ cmd: 'toggle_filter' }),
  setNoiseSuppression: (enabled: boolean) => send({ cmd: 'set_noise_suppression', enabled }),
} as const);

export type IpcActions = ReturnType<typeof useIpc>;
