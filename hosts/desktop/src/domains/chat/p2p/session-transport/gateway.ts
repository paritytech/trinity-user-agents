/**
 * Statement-store wire transport for P2P chat.
 *
 * Every submit here rides the app-policy `signAndSubmitStatement`
 * (`@/shared/statement-store`), which owns expiry allocation (so same-second
 * submits never tie on priority) and the AccountFull / ExpiryTooLow retry.
 *
 * What is left here after the 0.9.1 session migration: the submit policy itself, and
 * `postChatMessageOnDeviceChannel` — a one-shot send on a peer device's topic, used where
 * a full session would be too much (see its docblock).
 *
 * The channels themselves are sessions now, not one-shots on this gateway:
 * `../chatSessionV2.ts` (device-level) and `../identityChannel.ts` (identity-level).
 * Both acknowledge and retransmit, which the hand-rolled posts here never did.
 */

import { ChatMessage as ChatMessageCodec } from '@novasamatech/host-chat/codec/message';
import {
  type StatementStoreAdapter,
  createAccountId,
  createEncryption,
  createSessionId,
  khash,
} from '@novasamatech/statement-store';
import { nanoid } from 'nanoid';
import { type CodecType } from 'scale-ts';

import { signAndSubmitStatement } from '@/shared/statement-store';
import { StructuredStatementData } from '../requests/schemas';
import { p2pService } from '../service';

// ── Identity / device channel routing ────────────────────────────────────

const REQUEST_LABEL = new TextEncoder().encode('request');

type DeviceRoute = {
  ownIdentityChatPrivateKey: Uint8Array;
  peerIdentityAccountId: Uint8Array;
  peerIdentityChatPublicKey: Uint8Array;
  peerDeviceEncryptionPublicKey: Uint8Array;
  ownDeviceStatementAccountId: Uint8Array;
  ownDeviceEncryptionPrivateKey: Uint8Array;
};

/**
 * Per-peer-device "main" channel — matches `chatSessionV2.ts` outgoing
 * topic derivation (which is also what mobile expects on receive):
 *   topic = SessionId(D(A), B) = createSessionId(K(D(A),B), localDevice, remoteIdentity)
 * where:
 * - localDevice  = ownDeviceStatementAccountId
 * - remoteIdentity = peerIdentityAccountId
 * - K(D(A),B)  = ECDH(ownDeviceEncryptionPrivateKey, peerIdentityChatPublicKey)
 *
 * Used for steady-state fanout (deviceAdded/deviceRemoved) AFTER bootstrap
 * deviceChatAccepted on the identity-channel has established device knowledge
 * on both sides. Mobile subscribes on the same per-peer-device topic for both
 * regular chat traffic and roster updates — Android via
 * `communicationSessions.main`, iOS via `makePeerSubscription`. Posting roster
 * updates on this same topic means mobile already listens there.
 */
const computeDeviceRoute = (
  keys: DeviceRoute,
  direction: 'outgoing' | 'incoming',
): { topic: Uint8Array; sharedSecret: Uint8Array } => {
  const sharedSecret =
    direction === 'outgoing'
      ? p2pService.computeSharedSecret(keys.ownDeviceEncryptionPrivateKey, keys.peerIdentityChatPublicKey)
      : p2pService.computeSharedSecret(keys.ownIdentityChatPrivateKey, keys.peerDeviceEncryptionPublicKey);
  const localDevice = { accountId: createAccountId(keys.ownDeviceStatementAccountId), pin: undefined };
  const remoteIdentity = { accountId: createAccountId(keys.peerIdentityAccountId), pin: undefined };
  const topic =
    direction === 'outgoing'
      ? createSessionId(sharedSecret, localDevice, remoteIdentity)
      : createSessionId(sharedSecret, remoteIdentity, localDevice);
  return { topic, sharedSecret };
};

// ── Event surface (types in ./types) ────────────────────────────────────

// ── Send: ChatMessage(content) on a peer device's topic ─────────────────

/**
 * Per-peer-device "main" channel one-shot send, on the device-derived topic. Matches
 * mobile's `communicationSessions.main` (Android) / per-peer-device subscription (iOS).
 *
 * Wire is identical (StructuredStatementData::Request → ChatMessage), only the
 * outer envelope's ECDH key + topic differ.
 */
async function postChatMessageOnDeviceChannel(
  params: DeviceRoute & {
    signerDeviceSeed: Uint8Array;
    statementStore: StatementStoreAdapter;
    chatMessageContent: CodecType<typeof ChatMessageCodec>['versioned']['value'];
  },
): Promise<void> {
  const { signerDeviceSeed, statementStore, chatMessageContent, ...keys } = params;

  const chatMsgBytes = ChatMessageCodec.enc({
    messageId: nanoid(12),
    timestamp: BigInt(Date.now()),
    versioned: { tag: 'v1', value: chatMessageContent },
  });

  const payload = StructuredStatementData.enc({
    tag: 'Request',
    value: { requestId: nanoid(), messages: [chatMsgBytes] },
  });

  const { topic, sharedSecret } = computeDeviceRoute(keys, 'outgoing');
  const channel = khash(topic, REQUEST_LABEL);

  const encryption = createEncryption(sharedSecret);
  const encryptResult = encryption.encrypt(payload);
  if (encryptResult.isErr()) throw encryptResult.error;

  await signAndSubmitStatement({
    signerSeed: signerDeviceSeed,
    statementStore,
    channel,
    topics: topic,
    data: encryptResult.value,
    logTag: 'device-channel send',
  });
}

export const transportGateway = {
  signAndSubmitStatement,
  postChatMessageOnDeviceChannel,
};
