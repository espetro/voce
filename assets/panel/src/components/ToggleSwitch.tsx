import type { Component } from 'solid-js';
import { Switch } from '@ark-ui/solid/switch';

interface Props {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}

const ToggleSwitch: Component<Props> = (props) => (
  <Switch.Root
    checked={props.checked}
    disabled={props.disabled}
    onCheckedChange={(e) => props.onChange(e.checked)}
    class="toggle-row"
  >
    <Switch.Label class="toggle-label">{props.label}</Switch.Label>
    <Switch.Control class="switch-track">
      <Switch.Thumb class="switch-thumb" />
    </Switch.Control>
    <Switch.HiddenInput />
  </Switch.Root>
);

export default ToggleSwitch;
