import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type CallController, type CallControllerConfig, createCallController } from './callController';

const b = (n: number) => new Uint8Array([n]);

function makeConfig(overrides?: Partial<CallControllerConfig>): CallControllerConfig {
  return {
    launch: { direction: 'outgoing', purpose: 'video', offerId: 'o1' },
    sendToBridge: vi.fn(),
    applyRemoteAnswer: vi.fn(),
    addRemoteCandidates: vi.fn(),
    prepareAnswer: vi.fn(),
    startMediaUpgrade: vi.fn(),
    sendUpgradeRequest: vi.fn(),
    sendUpgradeDecline: vi.fn(),
    upgradeToVideo: vi.fn(),
    confirmVideoUpgrade: vi.fn(),
    sendMediaState: vi.fn(),
    setLocalTrackEnabled: vi.fn(),
    closePeerConnection: vi.fn(),
    closeWindow: vi.fn(),
    onStateChange: vi.fn(),
    ...overrides,
  };
}

describe('createCallController', () => {
  describe('initial state', () => {
    it('calls onStateChange once with the outgoing initial state (status=requesting)', () => {
      const config = makeConfig();
      createCallController(config);
      expect(config.onStateChange).toHaveBeenCalledOnce();
      expect(vi.mocked(config.onStateChange).mock.calls[0]![0].status).toBe('requesting');
    });

    it('calls onStateChange once with the incoming initial state (status=ringing)', () => {
      const config = makeConfig({ launch: { direction: 'incoming', purpose: 'audio', offerId: 'o1' } });
      createCallController(config);
      expect(config.onStateChange).toHaveBeenCalledOnce();
      expect(vi.mocked(config.onStateChange).mock.calls[0]![0].status).toBe('ringing');
    });

    it('getState returns the initial state', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      expect(ctrl.getState().status).toBe('requesting');
      expect(ctrl.getState().offerId).toBe('o1');
    });
  });

  describe('outgoing: localOfferEncoded → sendToBridge publishOffer', () => {
    it('onLocalOfferEncoded calls sendToBridge with publishOffer', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onLocalOfferEncoded(b(1));
      expect(config.sendToBridge).toHaveBeenCalledWith({ kind: 'publishOffer', offerId: 'o1', purpose: 'video', sdp: b(1) });
    });
  });

  describe('onBridgeMessage — deliverAnswer with matching offerId', () => {
    it('transitions to connecting and calls applyRemoteAnswer', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onBridgeMessage({ kind: 'deliverAnswer', offerId: 'o1', sdp: b(2) });
      expect(ctrl.getState().status).toBe('connecting');
      expect(config.applyRemoteAnswer).toHaveBeenCalledWith(b(2));
    });
  });

  describe('onBridgeMessage — deliverAnswer with NON-matching offerId', () => {
    it('does not call applyRemoteAnswer and leaves state unchanged', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      const stateBefore = ctrl.getState();
      ctrl.onBridgeMessage({ kind: 'deliverAnswer', offerId: 'other', sdp: b(2) });
      expect(config.applyRemoteAnswer).not.toHaveBeenCalled();
      expect(ctrl.getState()).toBe(stateBefore);
    });
  });

  describe('incoming: accept → connecting + prepareAnswer', () => {
    let config: CallControllerConfig;
    let ctrl: CallController;

    beforeEach(() => {
      config = makeConfig({ launch: { direction: 'incoming', purpose: 'audio', offerId: 'o1' } });
      ctrl = createCallController(config);
    });

    it('accept transitions to connecting and calls prepareAnswer', () => {
      ctrl.accept();
      expect(ctrl.getState().status).toBe('connecting');
      expect(config.prepareAnswer).toHaveBeenCalledOnce();
    });
  });

  describe('incoming: end → ended + sendToBridge publishClosed + closePeerConnection', () => {
    it('end from ringing produces ended status and expected side effects', () => {
      const config = makeConfig({ launch: { direction: 'incoming', purpose: 'audio', offerId: 'o1' } });
      const ctrl = createCallController(config);
      ctrl.end();
      expect(ctrl.getState().status).toBe('ended');
      expect(config.sendToBridge).toHaveBeenCalledWith({ kind: 'publishClosed', offerId: 'o1' });
      expect(config.closePeerConnection).toHaveBeenCalledOnce();
    });
  });

  describe('onDataChannelOpen → startMediaUpgrade', () => {
    it('calls startMediaUpgrade when data channel opens', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onDataChannelOpen();
      expect(config.startMediaUpgrade).toHaveBeenCalledOnce();
    });
  });

  describe('onMediaConnected → status connected', () => {
    it('transitions to connected after remoteAnswer + mediaConnected', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onBridgeMessage({ kind: 'deliverAnswer', offerId: 'o1', sdp: b(2) });
      ctrl.onMediaConnected();
      expect(ctrl.getState().status).toBe('connected');
    });
  });

  describe('toggleCamera → setLocalTrackEnabled + sendMediaState + state update', () => {
    it('toggleCamera(false) disables camera in state and calls both side effects', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.toggleCamera(false);
      expect(config.setLocalTrackEnabled).toHaveBeenCalledWith('camera', false);
      expect(config.sendMediaState).toHaveBeenCalledWith('camera', false);
      expect(ctrl.getState().media.cameraEnabled).toBe(false);
    });
  });

  describe('onStateChange fires on every state-changing dispatch', () => {
    it('fires again after each dispatch that changes state', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      // initial fire already counted (1)
      ctrl.onLocalOfferEncoded(b(1)); // state unchanged by reducer but onStateChange still fires
      ctrl.onBridgeMessage({ kind: 'deliverAnswer', offerId: 'o1', sdp: b(2) }); // → connecting
      ctrl.onMediaConnected(); // needs connecting→connected path; starts from connecting
      // 1 (init) + 3 dispatches = 4
      expect(config.onStateChange).toHaveBeenCalledTimes(4);
    });
  });

  describe('onBridgeMessage — deliverCandidates → addRemoteCandidates', () => {
    it('calls addRemoteCandidates with the candidates payload', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onBridgeMessage({ kind: 'deliverCandidates', offerId: 'o1', candidates: b(9) });
      expect(config.addRemoteCandidates).toHaveBeenCalledWith(b(9));
    });
  });

  describe('onBridgeMessage — deliverClosed → ended', () => {
    it('transitions to ended and calls closePeerConnection', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onBridgeMessage({ kind: 'deliverClosed', offerId: 'o1' });
      expect(ctrl.getState().status).toBe('ended');
      expect(config.closePeerConnection).toHaveBeenCalledOnce();
    });
  });

  describe('onBridgeMessage — provideIncomingCall is ignored', () => {
    it('does not change state on provideIncomingCall', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      const stateBefore = ctrl.getState();
      ctrl.onBridgeMessage({
        kind: 'provideIncomingCall',
        offerId: 'o1',
        purpose: 'video',
        sdp: b(1),
        candidates: b(2),
        peerName: 'Alice',
      });
      expect(ctrl.getState()).toBe(stateBefore);
    });
  });

  describe('onFailed → failed status + closePeerConnection', () => {
    it('onFailed transitions to failed and closes peer connection', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.onFailed();
      expect(ctrl.getState().status).toBe('failed');
      expect(config.closePeerConnection).toHaveBeenCalledOnce();
    });
  });

  describe('decline → ended (incoming)', () => {
    it('decline transitions to ended and closes with publishClosed', () => {
      const config = makeConfig({ launch: { direction: 'incoming', purpose: 'audio', offerId: 'o1' } });
      const ctrl = createCallController(config);
      ctrl.decline();
      expect(ctrl.getState().status).toBe('ended');
      expect(config.sendToBridge).toHaveBeenCalledWith({ kind: 'publishClosed', offerId: 'o1' });
      expect(config.closePeerConnection).toHaveBeenCalledOnce();
    });
  });

  describe('toggleMicrophone → setLocalTrackEnabled + sendMediaState', () => {
    it('toggleMicrophone(true) enables microphone in state and calls both side effects', () => {
      const config = makeConfig();
      const ctrl = createCallController(config);
      ctrl.toggleMicrophone(true);
      expect(config.setLocalTrackEnabled).toHaveBeenCalledWith('microphone', true);
      expect(config.sendMediaState).toHaveBeenCalledWith('microphone', true);
    });
  });

  describe('audio→video upgrade handshake', () => {
    // A connected audio call with the data channel open — the state in which an
    // upgrade may start on either side.
    function connectedAudioController(): { config: CallControllerConfig; ctrl: CallController } {
      const config = makeConfig({ launch: { direction: 'incoming', purpose: 'audio', offerId: 'o1' } });
      const ctrl = createCallController(config);
      ctrl.accept();
      ctrl.onDataChannelOpen();
      return { config, ctrl };
    }

    it('requestVideoUpgrade → sendUpgradeRequest, pendingUpgrade=outgoing, no camera yet', () => {
      const { config, ctrl } = connectedAudioController();
      ctrl.requestVideoUpgrade();
      expect(config.sendUpgradeRequest).toHaveBeenCalledOnce();
      expect(ctrl.getState().pendingUpgrade).toBe('outgoing');
      expect(ctrl.getState().media.cameraEnabled).toBe(false);
    });

    it('onRemoteUpgradeAccepted → upgradeToVideo + purpose video after our request', () => {
      const { config, ctrl } = connectedAudioController();
      ctrl.requestVideoUpgrade();
      ctrl.onRemoteUpgradeAccepted();
      expect(config.upgradeToVideo).toHaveBeenCalledOnce();
      expect(ctrl.getState().purpose).toBe('video');
      expect(ctrl.getState().pendingUpgrade).toBe('idle');
    });

    it('onRemoteUpgradeDeclined → clears pending, stays audio, no upgradeToVideo', () => {
      const { config, ctrl } = connectedAudioController();
      ctrl.requestVideoUpgrade();
      ctrl.onRemoteUpgradeDeclined();
      expect(config.upgradeToVideo).not.toHaveBeenCalled();
      expect(ctrl.getState().pendingUpgrade).toBe('idle');
      expect(ctrl.getState().purpose).toBe('audio');
    });

    it('onRemoteUpgradeRequested → pendingUpgrade=incoming (modal), no media yet', () => {
      const { ctrl } = connectedAudioController();
      ctrl.onRemoteUpgradeRequested();
      expect(ctrl.getState().pendingUpgrade).toBe('incoming');
      expect(ctrl.getState().media.cameraEnabled).toBe(false);
    });

    it('acceptVideoUpgrade → confirmVideoUpgrade, purpose video', () => {
      const { config, ctrl } = connectedAudioController();
      ctrl.onRemoteUpgradeRequested();
      ctrl.acceptVideoUpgrade();
      expect(config.confirmVideoUpgrade).toHaveBeenCalledOnce();
      expect(ctrl.getState().purpose).toBe('video');
      expect(ctrl.getState().pendingUpgrade).toBe('idle');
    });

    it('declineVideoUpgrade → sendUpgradeDecline, stays audio, no confirm', () => {
      const { config, ctrl } = connectedAudioController();
      ctrl.onRemoteUpgradeRequested();
      ctrl.declineVideoUpgrade();
      expect(config.sendUpgradeDecline).toHaveBeenCalledOnce();
      expect(config.confirmVideoUpgrade).not.toHaveBeenCalled();
      expect(ctrl.getState().purpose).toBe('audio');
      expect(ctrl.getState().pendingUpgrade).toBe('idle');
    });
  });
});
