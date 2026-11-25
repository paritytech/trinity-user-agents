/**
 * Audio→video upgrade consent handshake, multiplexed over the open data channel
 * under its own use-case id (parallel to the media-state and renegotiation
 * streams). Unlike the unilateral media renegotiation, the far peer must agree
 * before either side turns on a camera: the requester sends `request`, the peer
 * answers `accept` or `decline`, and only an `accept` triggers the in-band
 * video upgrade. Desktop↔desktop only — a peer that does not understand these
 * signals simply never accepts, so the call stays audio.
 */

import { Enum, _void } from 'scale-ts';

export const WEBRTC_UPGRADE_CONSENT_USE_CASE_ID = 'webrtc_upgrade_consent_use_case';

export const UpgradeConsentSignalCodec = Enum({
  request: _void, // 0
  accept: _void, // 1
  decline: _void, // 2
});

export type UpgradeConsentSignal = { tag: 'request' } | { tag: 'accept' } | { tag: 'decline' };
