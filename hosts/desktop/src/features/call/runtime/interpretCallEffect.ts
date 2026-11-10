import { type CallSessionEffect, type WindowToMainMessage } from '@/domains/chat';

export type CallEffectDeps = {
  sendToBridge: (message: WindowToMainMessage) => void;
  applyRemoteAnswer: (sdp: Uint8Array) => void;
  addRemoteCandidates: (candidates: Uint8Array) => void;
  prepareAnswer: () => void;
  startMediaUpgrade: () => void;
  sendUpgradeRequest: () => void;
  sendUpgradeDecline: () => void;
  upgradeToVideo: () => void;
  confirmVideoUpgrade: () => void;
  sendMediaState: (track: 'camera' | 'microphone', enabled: boolean) => void;
  setLocalTrackEnabled: (track: 'camera' | 'microphone', enabled: boolean) => void;
  closePeerConnection: () => void;
  closeWindow: () => void;
};

export function interpretCallEffect(effect: CallSessionEffect, deps: CallEffectDeps): void {
  switch (effect.type) {
    case 'publishOffer':
      deps.sendToBridge({ kind: 'publishOffer', offerId: effect.offerId, purpose: effect.purpose, sdp: effect.sdp });
      break;
    case 'publishAnswer':
      deps.sendToBridge({ kind: 'publishAnswer', offerId: effect.offerId, sdp: effect.sdp });
      break;
    case 'publishCandidates':
      deps.sendToBridge({ kind: 'publishCandidates', offerId: effect.offerId, candidates: effect.candidates });
      break;
    case 'publishClosed':
      deps.sendToBridge({ kind: 'publishClosed', offerId: effect.offerId });
      break;
    case 'prepareAnswer':
      deps.prepareAnswer();
      break;
    case 'applyRemoteAnswer':
      deps.applyRemoteAnswer(effect.sdp);
      break;
    case 'addRemoteCandidates':
      deps.addRemoteCandidates(effect.candidates);
      break;
    case 'startMediaUpgrade':
      deps.startMediaUpgrade();
      break;
    case 'sendUpgradeRequest':
      deps.sendUpgradeRequest();
      break;
    case 'sendUpgradeDecline':
      deps.sendUpgradeDecline();
      break;
    case 'upgradeToVideo':
      deps.upgradeToVideo();
      break;
    case 'confirmVideoUpgrade':
      deps.confirmVideoUpgrade();
      break;
    case 'sendMediaState':
      deps.sendMediaState(effect.track, effect.enabled);
      break;
    case 'setLocalTrackEnabled':
      deps.setLocalTrackEnabled(effect.track, effect.enabled);
      break;
    case 'closePeerConnection':
      deps.closePeerConnection();
      break;
    case 'closeWindow':
      deps.closeWindow();
      break;
    default:
      break;
  }
}
