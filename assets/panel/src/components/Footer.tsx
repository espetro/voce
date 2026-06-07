import type { Component } from 'solid-js';
import { Show } from 'solid-js';
import type { Screen } from '../types';

const LABELS = ['Record your voice', 'Second sample', 'Test the filter'];

function screenToStep(screen: Screen): number | null {
  switch (screen) {
    case 'onboarding-ready':
      case 'recording-1':
        case 'recording-1-invalid':
          return 1

        case 'recording-1-complete':
          case 'recording-2':
            case 'recording-2-invalid':
              return 2

            case 'test-ready':
              case 'testing':
                case 'playback-filtered':
                  case 'playback-raw':
                    case 'test-complete':
                      return 3

                      default:
                        return null
  }
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
