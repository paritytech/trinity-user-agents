export type CallDeviceKind = 'camera' | 'microphone' | 'output';

export type CallDeviceSelection = {
  cameraId: Nullable<string>;
  microphoneId: Nullable<string>;
  outputId: Nullable<string>;
};

export type CallDevices = {
  cameras: MediaDeviceInfo[];
  microphones: MediaDeviceInfo[];
  outputs: MediaDeviceInfo[];
};
