import {
  type CallDirection,
  type CallPurpose,
  type CallSessionEvent,
  type CallSessionState,
  type MainToWindowMessage,
  type WindowToMainMessage,
  callSessionService,
} from '@/domains/chat';

import { type CallEffectDeps, interpretCallEffect } from './interpretCallEffect';

export type CallControllerConfig = {
  launch: { direction: CallDirection; purpose: CallPurpose; offerId: string };
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
  onStateChange: (state: CallSessionState) => void;
};

export type CallController = {
  getState: () => CallSessionState;
  dispatch: (event: CallSessionEvent) => void;
  accept: () => void;
  decline: () => void;
  end: () => void;
  toggleCamera: (enabled: boolean) => void;
  toggleMicrophone: (enabled: boolean) => void;
  requestVideoUpgrade: () => void;
  acceptVideoUpgrade: () => void;
  declineVideoUpgrade: () => void;
  onRemoteUpgradeRequested: () => void;
  onRemoteUpgradeAccepted: () => void;
  onRemoteUpgradeDeclined: () => void;
  onLocalOfferEncoded: (sdp: Uint8Array) => void;
  onLocalAnswerEncoded: (sdp: Uint8Array) => void;
  onLocalCandidatesEncoded: (candidates: Uint8Array) => void;
  onDataChannelOpen: () => void;
  onMediaConnected: () => void;
  onFailed: () => void;
  onBridgeMessage: (message: MainToWindowMessage) => void;
};

export function createCallController(config: CallControllerConfig): CallController {
  let state = callSessionService.initCallSession(config.launch);

  const effectDeps: CallEffectDeps = {
    sendToBridge: config.sendToBridge,
    applyRemoteAnswer: config.applyRemoteAnswer,
    addRemoteCandidates: config.addRemoteCandidates,
    prepareAnswer: config.prepareAnswer,
    startMediaUpgrade: config.startMediaUpgrade,
    sendUpgradeRequest: config.sendUpgradeRequest,
    sendUpgradeDecline: config.sendUpgradeDecline,
    upgradeToVideo: config.upgradeToVideo,
    confirmVideoUpgrade: config.confirmVideoUpgrade,
    sendMediaState: config.sendMediaState,
    setLocalTrackEnabled: config.setLocalTrackEnabled,
    closePeerConnection: config.closePeerConnection,
    closeWindow: config.closeWindow,
  };

  config.onStateChange(state);

  function dispatch(event: CallSessionEvent): void {
    const result = callSessionService.callSessionReducer(state, event);
    state = result.state;
    config.onStateChange(state);
    for (const effect of result.effects) {
      interpretCallEffect(effect, effectDeps);
    }
  }

  function onBridgeMessage(message: MainToWindowMessage): void {
    switch (message.kind) {
      case 'deliverAnswer':
        dispatch({ type: 'remoteAnswer', offerId: message.offerId, sdp: message.sdp });
        break;
      case 'deliverCandidates':
        dispatch({ type: 'remoteCandidates', offerId: message.offerId, candidates: message.candidates });
        break;
      case 'deliverClosed':
        dispatch({ type: 'remoteClosed', offerId: message.offerId });
        break;
      case 'dismissAnsweredElsewhere':
        dispatch({ type: 'answeredElsewhere', offerId: message.offerId });
        break;
      case 'provideIncomingCall':
        // used at launch construction — ignore here
        break;
      default:
        break;
    }
  }

  return {
    getState: () => state,
    dispatch,
    accept: () => dispatch({ type: 'accept' }),
    decline: () => dispatch({ type: 'decline' }),
    end: () => dispatch({ type: 'end' }),
    toggleCamera: (enabled: boolean) => dispatch({ type: 'toggleMedia', track: 'camera', enabled }),
    toggleMicrophone: (enabled: boolean) => dispatch({ type: 'toggleMedia', track: 'microphone', enabled }),
    requestVideoUpgrade: () => dispatch({ type: 'requestVideoUpgrade' }),
    acceptVideoUpgrade: () => dispatch({ type: 'acceptVideoUpgrade' }),
    declineVideoUpgrade: () => dispatch({ type: 'declineVideoUpgrade' }),
    onRemoteUpgradeRequested: () => dispatch({ type: 'remoteUpgradeRequested' }),
    onRemoteUpgradeAccepted: () => dispatch({ type: 'remoteUpgradeAccepted' }),
    onRemoteUpgradeDeclined: () => dispatch({ type: 'remoteUpgradeDeclined' }),
    onLocalOfferEncoded: (sdp: Uint8Array) => dispatch({ type: 'localOfferEncoded', sdp }),
    onLocalAnswerEncoded: (sdp: Uint8Array) => dispatch({ type: 'localAnswerEncoded', sdp }),
    onLocalCandidatesEncoded: (candidates: Uint8Array) => dispatch({ type: 'localCandidatesEncoded', candidates }),
    onDataChannelOpen: () => dispatch({ type: 'dataChannelOpen' }),
    onMediaConnected: () => dispatch({ type: 'mediaConnected' }),
    onFailed: () => dispatch({ type: 'failed' }),
    onBridgeMessage,
  };
}
