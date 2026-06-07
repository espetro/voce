import type { Component } from 'solid-js';
import { Show } from 'solid-js';
import type { Screen } from '../types';

const LABELS = ['Record your voice', 'Second sample', 'Test the filter'];

function screenToStep(screen: Screen): number | null {
  if (['onboarding-ready', 'recording-1', 'recording-1-invalid'].includes(screen)) return 1;
  if (['recording-1-complete', 'recording-2', 'recording-2-invalid'].includes(screen)) return 2;
  if (['test-ready', 'testing', 'playback-filtered', 'playback-raw', 'test-complete'].includes(screen)) return 3;
  return null;
}

const Footer: Component<{ screen: Screen }> = (props) => {
  const step = () => screenToStep(props.screen);

  return (
    <Show when={step() !== null}>
      <div class="footer">
        {[1, 2, 3].map((n) => (
          <div class={step() === n ? 'step-dot step-dot-active' : 'step-dot'} />
        ))}
        <span class="step-text">
          Step {step()} of 3: {LABELS[(step() ?? 1) - 1]}
        </span>
      </div>
    </Show>
  );
};

export default Footer;
