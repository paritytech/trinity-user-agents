import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type CallEffectDeps, interpretCallEffect } from './interpretCallEffect';

function makeDeps(): CallEffectDeps {
  return {
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
  };
}

describe('interpretCallEffect', () => {
  let deps: CallEffectDeps;

  beforeEach(() => {
    deps = makeDeps();
  });

  it('publishOffer — sends the offer to bridge', () => {
    const sdp = new Uint8Array([1, 2, 3]);
    interpretCallEffect({ type: 'publishOffer', offerId: 'o1', purpose: 'audio', sdp }, deps);
    expect(deps.sendToBridge).toHaveBeenCalledOnce();
    expect(deps.sendToBridge).toHaveBeenCalledWith({ kind: 'publishOffer', offerId: 'o1', purpose: 'audio', sdp });
  });

  it('publishOffer — does not call applyRemoteAnswer (routing isolation)', () => {
    const sdp = new Uint8Array([1, 2, 3]);
    interpretCallEffect({ type: 'publishOffer', offerId: 'o1', purpose: 'audio', sdp }, deps);
    expect(deps.applyRemoteAnswer).not.toHaveBeenCalled();
  });

  it('publishAnswer — sends the answer to bridge', () => {
    const sdp = new Uint8Array([4, 5, 6]);
    interpretCallEffect({ type: 'publishAnswer', offerId: 'o1', sdp }, deps);
    expect(deps.sendToBridge).toHaveBeenCalledOnce();
    expect(deps.sendToBridge).toHaveBeenCalledWith({ kind: 'publishAnswer', offerId: 'o1', sdp });
  });

  it('publishCandidates — sends candidates to bridge', () => {
    const candidates = new Uint8Array([7, 8, 9]);
    interpretCallEffect({ type: 'publishCandidates', offerId: 'o1', candidates }, deps);
    expect(deps.sendToBridge).toHaveBeenCalledOnce();
    expect(deps.sendToBridge).toHaveBeenCalledWith({ kind: 'publishCandidates', offerId: 'o1', candidates });
  });

  it('publishClosed — sends closed to bridge', () => {
    interpretCallEffect({ type: 'publishClosed', offerId: 'o1' }, deps);
    expect(deps.sendToBridge).toHaveBeenCalledOnce();
    expect(deps.sendToBridge).toHaveBeenCalledWith({ kind: 'publishClosed', offerId: 'o1' });
  });

  it('prepareAnswer — calls prepareAnswer with no args', () => {
    interpretCallEffect({ type: 'prepareAnswer' }, deps);
    expect(deps.prepareAnswer).toHaveBeenCalledOnce();
    expect(deps.prepareAnswer).toHaveBeenCalledWith();
  });

  it('applyRemoteAnswer — passes sdp to applyRemoteAnswer', () => {
    const sdp = new Uint8Array([10, 11]);
    interpretCallEffect({ type: 'applyRemoteAnswer', sdp }, deps);
    expect(deps.applyRemoteAnswer).toHaveBeenCalledOnce();
    expect(deps.applyRemoteAnswer).toHaveBeenCalledWith(sdp);
  });

  it('addRemoteCandidates — passes candidates to addRemoteCandidates', () => {
    const candidates = new Uint8Array([12, 13, 14]);
    interpretCallEffect({ type: 'addRemoteCandidates', candidates }, deps);
    expect(deps.addRemoteCandidates).toHaveBeenCalledOnce();
    expect(deps.addRemoteCandidates).toHaveBeenCalledWith(candidates);
  });

  it('startMediaUpgrade — calls startMediaUpgrade once', () => {
    interpretCallEffect({ type: 'startMediaUpgrade' }, deps);
    expect(deps.startMediaUpgrade).toHaveBeenCalledOnce();
  });

  it('sendUpgradeRequest — calls sendUpgradeRequest once', () => {
    interpretCallEffect({ type: 'sendUpgradeRequest' }, deps);
    expect(deps.sendUpgradeRequest).toHaveBeenCalledOnce();
  });

  it('sendUpgradeDecline — calls sendUpgradeDecline once', () => {
    interpretCallEffect({ type: 'sendUpgradeDecline' }, deps);
    expect(deps.sendUpgradeDecline).toHaveBeenCalledOnce();
  });

  it('upgradeToVideo — calls upgradeToVideo once', () => {
    interpretCallEffect({ type: 'upgradeToVideo' }, deps);
    expect(deps.upgradeToVideo).toHaveBeenCalledOnce();
  });

  it('confirmVideoUpgrade — calls confirmVideoUpgrade once', () => {
    interpretCallEffect({ type: 'confirmVideoUpgrade' }, deps);
    expect(deps.confirmVideoUpgrade).toHaveBeenCalledOnce();
  });

  it('sendMediaState — passes camera/true to sendMediaState', () => {
    interpretCallEffect({ type: 'sendMediaState', track: 'camera', enabled: true }, deps);
    expect(deps.sendMediaState).toHaveBeenCalledOnce();
    expect(deps.sendMediaState).toHaveBeenCalledWith('camera', true);
  });

  it('setLocalTrackEnabled — passes microphone/false to setLocalTrackEnabled', () => {
    interpretCallEffect({ type: 'setLocalTrackEnabled', track: 'microphone', enabled: false }, deps);
    expect(deps.setLocalTrackEnabled).toHaveBeenCalledOnce();
    expect(deps.setLocalTrackEnabled).toHaveBeenCalledWith('microphone', false);
  });

  it('closePeerConnection — calls closePeerConnection once', () => {
    interpretCallEffect({ type: 'closePeerConnection' }, deps);
    expect(deps.closePeerConnection).toHaveBeenCalledOnce();
  });

  it('closeWindow — calls closeWindow once', () => {
    interpretCallEffect({ type: 'closeWindow' }, deps);
    expect(deps.closeWindow).toHaveBeenCalledOnce();
  });
});
