// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { type CallDevices } from '@/shared/call-media-devices';

import { CallSettings } from './CallSettings';

const info = (kind: MediaDeviceKind, deviceId: string, label: string): MediaDeviceInfo =>
  ({ kind, deviceId, label, groupId: '', toJSON: () => ({}) }) as MediaDeviceInfo;

const devices: CallDevices = {
  cameras: [info('videoinput', 'cam-1', 'FaceTime HD'), info('videoinput', 'cam-2', 'External Cam')],
  microphones: [info('audioinput', 'mic-1', 'Built-in Mic')],
  outputs: [info('audiooutput', 'out-1', 'Speakers')],
};

const labels = {
  title: 'Call settings',
  close: 'Close',
  camera: 'Camera',
  microphone: 'Microphone',
  output: 'Output',
  systemDefault: 'Default',
};

const setup = (overrides: Partial<Parameters<typeof CallSettings>[0]> = {}) => {
  const onSelect = vi.fn();
  const onClose = vi.fn();
  render(
    <CallSettings
      devices={devices}
      selection={{ cameraId: 'cam-2', microphoneId: null, outputId: null }}
      labels={labels}
      onSelect={onSelect}
      onClose={onClose}
      {...overrides}
    />,
  );
  return { onSelect, onClose };
};

describe('CallSettings', () => {
  it('renders the three category rows', () => {
    setup();
    expect(screen.getByRole('button', { name: 'Camera' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Microphone' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Output' })).toBeDefined();
  });

  it('shows the selected camera name as the Camera sublabel, "Default" for unset kinds', () => {
    setup();
    expect(screen.getByText('External Cam')).toBeDefined(); // cam-2 selected
    // Microphone + Output are unset → both show the default label (2 occurrences).
    expect(screen.getAllByText('Default').length).toBe(2);
  });

  it('fires onClose when the close button is pressed', async () => {
    const { onClose } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('opens a device popover on the row and marks the selected device', async () => {
    setup();
    // Popover is closed initially.
    expect(screen.queryByRole('menu')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Camera' }));
    expect(screen.getByRole('menu')).toBeDefined();
    expect(screen.getByRole('menuitemradio', { name: 'FaceTime HD' })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByRole('menuitemradio', { name: 'External Cam' })).toHaveAttribute('aria-checked', 'true');
  });

  it('picks a device from the popover and closes it', async () => {
    const { onSelect } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Camera' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: 'FaceTime HD' }));
    expect(onSelect).toHaveBeenCalledWith('camera', 'cam-1');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('offers a system-default entry in the output popover', async () => {
    const { onSelect } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Output' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: 'Default' }));
    expect(onSelect).toHaveBeenCalledWith('output', null);
  });

  it('closes an open popover when clicking the backdrop', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Microphone' }));
    expect(screen.getByRole('menu')).toBeDefined();
    // The backdrop is the outermost element; clicking it dismisses the popover.
    const backdrop = document.querySelector('.bg-black\\/60');
    if (backdrop instanceof HTMLElement) await userEvent.click(backdrop);
    expect(screen.queryByRole('menu')).toBeNull();
  });
});
