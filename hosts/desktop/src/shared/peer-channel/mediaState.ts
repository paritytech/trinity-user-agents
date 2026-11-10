/**
 * Mid-call media-state broadcast over the data channel. Parity with Android
 * `MediaStateSignal.kt` (CameraEnabled=0 / MicrophoneEnabled=1, each a bool).
 * Multiplexed under its own use-case id, separate from the renegotiation
 * signals. iOS does not emit this (it toggles tracks locally); receiving is
 * therefore best-effort and backward-compatible.
 */

import { Enum, bool } from 'scale-ts';

export const WEBRTC_MEDIA_STATE_USE_CASE_ID = 'webrtc_media_state_use_case';

export const MediaStateSignalCodec = Enum({
  cameraEnabled: bool, // 0
  microphoneEnabled: bool, // 1
});

export type MediaStateSignal = { tag: 'cameraEnabled'; value: boolean } | { tag: 'microphoneEnabled'; value: boolean };
