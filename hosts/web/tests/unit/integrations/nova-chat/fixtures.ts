import * as sr25519 from "@scure/sr25519";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { x25519PublicKey } from "../../../../bridges/primitives/crypto.js";
import {
  decodeStatement,
  encodeSigned,
  type SignedStatement,
  type Statement,
} from "../../../../bridges/primitives/statement.js";
import {
  aeadKey,
  agree,
  seal,
} from "../../../../src/integrations/nova-chat/crypto.js";
import { NovaBoundary } from "../../../../src/integrations/nova-chat/boundary.js";
import {
  requestChannel,
  sessionTopic,
} from "../../../../src/integrations/nova-chat/topics.js";
import {
  type ChatMessage,
  encodeFrameBody,
  encodeMessage,
  encodeStatementData,
} from "../../../../bridges/nova-chat/wire.js";

/** Synthetic keys only. B's chat and device secrets are the Python peer's fixed `[3]*32` and `[4]*32`. */
export const seed = (byte: number) => new Uint8Array(32).fill(byte);

export const A = {
  identitySecret: sr25519.secretFromSeed(seed(0x11)),
  chatSecret: seed(0x21),
  deviceSecret: seed(0x22),
};
export const A_ID = sr25519.getPublicKey(A.identitySecret);
export const A_CHAT = x25519PublicKey(A.chatSecret);
export const A_DEVICE = x25519PublicKey(A.deviceSecret);

export const B_IDENTITY_SECRET = sr25519.secretFromSeed(seed(0x12));
export const B_ID = sr25519.getPublicKey(B_IDENTITY_SECRET);
export const B_CHAT_SECRET = seed(3);
export const B_DEVICE_SECRET = seed(4);
export const B_CHAT = x25519PublicKey(B_CHAT_SECRET);
export const B_DEVICE = x25519PublicKey(B_DEVICE_SECRET);
export const B = {
  identitySecret: B_IDENTITY_SECRET,
  chatSecret: B_CHAT_SECRET,
  deviceSecret: B_DEVICE_SECRET,
};

/** B's per-chat statement account (the apps' `//peer-statement//<peer>`): any key but B's device account. */
export const B_PER_CHAT_SECRET = sr25519.secretFromSeed(seed(0x13));
/** A notification account that signs requests and acceptances. */
export const NOTIFICATION_SECRET = sr25519.secretFromSeed(seed(0x14));
/** A conversation account. */
export const CONVERSATION_SECRET = sr25519.secretFromSeed(seed(0x15));

export const hex = (bytes: Uint8Array): string =>
  bytesToHex(bytes).toLowerCase();
export const A_PEER = hex(A_ID);
export const B_PEER = hex(B_ID);

/** The chain's chat keys, with `overrides` by account hex. */
export function chatKeys(overrides: [string, Uint8Array][] = []) {
  const keys = new Map<string, Uint8Array>([
    [A_PEER, A_CHAT],
    [B_PEER, B_CHAT],
    ...overrides,
  ]);
  return async (identity: Uint8Array) => keys.get(hex(identity)) ?? null;
}

export const boundaryA = (chatKeyOf = chatKeys()) =>
  new NovaBoundary(A, chatKeyOf);
export const boundaryB = (chatKeyOf = chatKeys()) =>
  new NovaBoundary(B, chatKeyOf);

/** Sign `statement` as `secret` and decode it back, as a subscriber sees it. */
export const signAs = (
  secret: Uint8Array,
  statement: Statement,
): SignedStatement =>
  decodeStatement(
    encodeSigned(statement, sr25519.getPublicKey(secret), (payload) =>
      sr25519.sign(secret, payload),
    ),
  );

/** A statement from the Python peer's JSON (`topic`, `channel`, `data`). */
export const fromPeer = (entry: {
  topic: string;
  channel: string;
  data: string;
}): Statement => ({
  expiry: 1n,
  channel: hexToBytes(entry.channel as HexString),
  topics: [hexToBytes(entry.topic as HexString)],
  data: hexToBytes(entry.data as HexString),
});

export const asPeerJson = (statement: SignedStatement) => ({
  topics: statement.topics.map(hex),
  channel: statement.channel === null ? null : hex(statement.channel),
  data: hex(statement.data),
});

/** B -> A on the identity route: a plain request under ECDH(B chat, A chat). */
export function bIdentityStatement(
  messages: ChatMessage[],
  requestId: string,
): Statement {
  const shared = agree(B_CHAT_SECRET, A_CHAT);
  const topic = sessionTopic(shared, B_ID, A_ID);
  return {
    expiry: 1n,
    channel: requestChannel(topic),
    topics: [topic],
    data: seal(
      aeadKey(shared),
      encodeStatementData({
        kind: "Request",
        requestId,
        messages: messages.map(encodeMessage),
      }),
    ),
  };
}

/** B's kind 20 for A's request `requestId`. */
export const bAcceptance = (requestId: string, deviceKey = B_DEVICE) =>
  bIdentityStatement(
    [
      {
        id: "b-acc",
        timestamp: 1n,
        content: {
          kind: "DeviceChatAccepted",
          requestId,
          device: { statementAccount: B_ID, encryptionKey: deviceKey },
        },
      },
    ],
    "b-acc",
  );

/** B's device -> A on the device route: a multi-device request with one entry for `recipient`. */
export function bDeviceStatement(
  messages: Uint8Array[],
  requestId: string,
  recipient: Uint8Array = A_ID,
): Statement {
  const outer = agree(B_DEVICE_SECRET, A_CHAT);
  const topic = sessionTopic(outer, B_ID, A_ID);
  const payloadKey = seed(0x77);
  return {
    expiry: 1n,
    channel: requestChannel(topic),
    topics: [topic],
    data: seal(
      aeadKey(outer),
      encodeStatementData({
        kind: "MultiRequest",
        payload: seal(
          payloadKey,
          encodeFrameBody({ kind: "Request", requestId, messages }),
        ),
        devices: [
          {
            statementAccount: recipient,
            encryptedKey: seal(
              aeadKey(agree(B_DEVICE_SECRET, A_DEVICE)),
              payloadKey,
            ),
          },
        ],
      }),
    ),
  };
}

export const text = (id: string, value: string) =>
  encodeMessage({ id, timestamp: 1n, content: { kind: "Text", text: value } });

/** A with an established chat with B, after A's request and B's acceptance. */
export async function establishedA(): Promise<NovaBoundary> {
  const host = boundaryA();
  const sent = await host.request(B_ID, {
    requestId: "a-req",
    nowMs: 1_800_000_300_000,
    expiry: 1n,
    welcome: null,
  });
  if ("refused" in sent) throw new Error(sent.refused);
  const accepted = host.openAcceptance(
    B_PEER,
    signAs(B_PER_CHAT_SECRET, bAcceptance("a-req")),
  );
  if ("refused" in accepted) throw new Error(accepted.refused);
  return host;
}
