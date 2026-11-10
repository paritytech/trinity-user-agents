import * as sr25519 from "@scure/sr25519";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import {
  Reader,
  equalBytes,
  text,
  utf8,
} from "../../../bridges/primitives/codec.js";
import {
  cryptoRandom,
  keyedBlake2b256,
  x25519PublicKey,
} from "../../../bridges/primitives/crypto.js";
import {
  type SignedStatement,
  type Statement,
  verifyStatement,
} from "../../../bridges/primitives/statement.js";
import {
  chainCode,
  deriveChatSecret,
  deriveIdentitySecret,
} from "../../invitations/chat-keys.js";
import { aeadKey, agree, open, seal } from "./crypto.js";
import {
  protocolDay,
  requestChannel,
  requestDayTopic,
  requestFullTopic,
  requestSessionTopic,
  responseChannel,
  sessionTopic,
} from "./topics.js";
import {
  type ChatMessage,
  type DeviceInfo,
  type Frame,
  decodeFrameBody,
  decodeMessage,
  decodeRequestEnvelope,
  decodeStatementData,
  encodeFrameBody,
  encodeMessage,
  encodeRequestDecrypted,
  encodeRequestEnvelope,
  encodeRequestMessage,
  encodeStatementData,
  identityProofPayload,
  isUserContent,
  readRequestMessage,
  requestProofPayload,
} from "../../../bridges/nova-chat/wire.js";

/**
 * PROTOTYPE. Nova Chat's host key boundary for one wallet: every step of the
 * official apps' chat protocol that needs a private key. It holds the
 * identity's sr25519 key, its chat X25519 secret and this device's X25519
 * secret, and every secret made from them; none leaves it. It answers with
 * unsigned statements, topics and checked frames.
 *
 * The browser is its identity's primary device, so the device's statement
 * account is the identity account, as on the apps' wallet device (Android
 * `RealOurDevicesProvider.kt`, iOS `ChatRequestStoreService.swift`). Peers
 * are kept by their identity account, `0x` hex, lowercase.
 *
 * What it checks of what it reads: the statement's sr25519 proof by any
 * signer, then the exact topic it expects, the encryption, the request's
 * inner signature and identity proof against the sender's on-chain chat key,
 * and a strict layout. A sender is bound by topic, never by signer, as both
 * apps do (Android `RealIncomingStatementsStream.kt`, iOS `PeerSession.swift`).
 */

export type PeerId = string;

export type Refused = { refused: string };
const refuse = (reason: string): Refused => ({ refused: reason });
export const isRefused = (value: object): value is Refused =>
  "refused" in value;

/** People `Resources.Consumers[account].identifierKey` as an X25519 key, or null. */
export type ChatKeyOf = (identity: Uint8Array) => Promise<Uint8Array | null>;

type PeerState = "outgoing" | "incoming" | "established";

interface Peer {
  identity: Uint8Array;
  chatKey: Uint8Array;
  state: PeerState;
  /** Our pending request's id (outgoing) or theirs (incoming and established). */
  requestId: string;
  devices: DeviceInfo[];
}

/** A frame as the worker receives it: user content, and a stub for any other kind. */
export type WorkerFrame =
  | { kind: "Request"; requestId: string; messages: ChatMessage[] }
  | { kind: "Response"; requestId: string; code: number };

/** An opened request from the request topics. */
export interface OpenedRequest {
  peer: PeerId;
  requestId: string;
  welcome: string | null;
  /** The same request was opened before. */
  duplicate: boolean;
}

/** The apps refuse a request id with `/`, since they derive accounts from it (iOS `ChatKeyPaths.validateRequestId`). */
const REQUEST_ID = /^[^/]+$/;

/** The device secret: RFC-0022 ECDH key material of the `chat-device` domain, as the chat key is of `chat`. */
export function deriveDeviceSecret(entropy: Uint8Array): Uint8Array {
  const root = keyedBlake2b256(utf8("ecdh"), entropy);
  return keyedBlake2b256(chainCode(text("chat-device")), root);
}

const peerId = (identity: Uint8Array): PeerId =>
  bytesToHex(identity).toLowerCase();

function usableKey(key: Uint8Array): boolean {
  if (key.length !== 32) return false;
  try {
    agree(new Uint8Array(32).fill(1), key);
    return true;
  } catch {
    return false;
  }
}

export class NovaBoundary {
  readonly identity: Uint8Array;
  readonly chatPublic: Uint8Array;
  readonly devicePublic: Uint8Array;
  readonly #identitySecret: Uint8Array;
  readonly #chatSecret: Uint8Array;
  readonly #deviceSecret: Uint8Array;
  private readonly peers = new Map<PeerId, Peer>();

  constructor(
    secrets: {
      identitySecret: Uint8Array;
      chatSecret: Uint8Array;
      deviceSecret: Uint8Array;
    },
    private readonly chatKeyOf: ChatKeyOf,
    private readonly random: (length: number) => Uint8Array = cryptoRandom,
  ) {
    this.#identitySecret = secrets.identitySecret;
    this.#chatSecret = secrets.chatSecret;
    this.#deviceSecret = secrets.deviceSecret;
    this.identity = sr25519.getPublicKey(secrets.identitySecret);
    this.chatPublic = x25519PublicKey(secrets.chatSecret);
    this.devicePublic = x25519PublicKey(secrets.deviceSecret);
  }

  /** The boundary of the wallet whose BIP-39 entropy is `entropy`, on the network with identity suffix `networkSuffix`. */
  static fromEntropy(
    entropy: Uint8Array,
    networkSuffix: string,
    chatKeyOf: ChatKeyOf,
  ): NovaBoundary {
    return new NovaBoundary(
      {
        identitySecret: deriveIdentitySecret(entropy, networkSuffix),
        chatSecret: deriveChatSecret(entropy),
        deviceSecret: deriveDeviceSecret(entropy),
      },
      chatKeyOf,
    );
  }

  /** Where `peer` stands, or null for someone this boundary does not know. */
  state(peer: PeerId): PeerState | null {
    return this.peers.get(peer)?.state ?? null;
  }

  /** The exact setup request bound to `peer`, or null when none is kept. */
  requestId(peer: PeerId): string | null {
    return this.peers.get(peer)?.requestId ?? null;
  }

  /** Everyone this boundary knows, by identity account. */
  known(): PeerId[] {
    return [...this.peers.keys()];
  }

  /** The topics the host reads for requests to this identity on `nowMs`: today's and yesterday's day topics, and the full topic. */
  requestTopics(nowMs: number): Uint8Array[] {
    const day = protocolDay(nowMs);
    return [
      requestDayTopic(this.identity, day),
      ...(day > 0n ? [requestDayTopic(this.identity, day - 1n)] : []),
      requestFullTopic(this.identity),
    ];
  }

  // ---- Requests out -----------------------------------------------------------

  /**
   * A V2 request to `identity` (Android `ChatRequestRemoteModel.kt`, iOS
   * `ChatRequestFactory.swift`): the identity proof, our device key, no push
   * token, the welcome, the inner signature by our device account over
   * (message, acceptor), sealed with a fresh X25519 key to the acceptor's
   * on-chain chat key, on the day, full and pair topics. Unsigned: the host
   * signs it with a claimed notification account.
   */
  async request(
    identity: Uint8Array,
    options: {
      requestId: string;
      nowMs: number;
      expiry: bigint;
      welcome: string | null;
    },
  ): Promise<{ peer: PeerId; statement: Statement } | Refused> {
    if (!REQUEST_ID.test(options.requestId)) return refuse("request id");
    if (equalBytes(identity, this.identity))
      return refuse("this is your own identity");
    const id = peerId(identity);
    const known = this.peers.get(id);
    if (known !== undefined && known.state !== "outgoing")
      return refuse(
        known.state === "incoming"
          ? "they already sent you a request: accept it in your Inbox"
          : "you already chat with them",
      );
    const chatKey = await this.chatKeyOf(identity);
    if (chatKey === null || !usableKey(chatKey))
      return refuse("they have no chat key on chain");
    const shared = agree(this.#chatSecret, chatKey);
    const message = encodeRequestMessage({
      messageId: options.requestId,
      timestamp: BigInt(options.nowMs),
      identityAccount: this.identity,
      identityProof: keyedBlake2b256(
        shared,
        identityProofPayload(this.identity, this.identity),
      ),
      deviceEncryptionKey: this.devicePublic,
      welcome: options.welcome,
    });
    const signature = sr25519.sign(
      this.#identitySecret,
      requestProofPayload(message, identity),
    );
    const ephemeral = this.random(32);
    const data = encodeRequestEnvelope(
      x25519PublicKey(ephemeral),
      seal(
        aeadKey(agree(ephemeral, chatKey)),
        encodeRequestDecrypted(message, signature, this.identity),
      ),
    );
    this.peers.set(id, {
      identity,
      chatKey,
      state: "outgoing",
      requestId: options.requestId,
      devices: [],
    });
    return {
      peer: id,
      statement: {
        expiry: options.expiry,
        channel: null,
        topics: [
          requestDayTopic(identity, protocolDay(options.nowMs)),
          requestFullTopic(identity),
          requestSessionTopic(shared, this.identity, identity),
        ],
        data,
      },
    };
  }

  /** Forget our pending request to `peer`. Nothing is sent: the apps keep a request until it lapses. */
  withdraw(peer: PeerId): boolean {
    if (this.peers.get(peer)?.state !== "outgoing") return false;
    this.peers.delete(peer);
    return true;
  }

  // ---- Requests in --------------------------------------------------------------

  /**
   * Opens a statement from our request topics: the statement proof, that it
   * opens with our chat key, the inner signature over the request and our
   * account, and the identity proof against the sender's on-chain chat key.
   * The caller drops requests from blocked people before it records them.
   */
  async openRequest(
    statement: SignedStatement,
  ): Promise<OpenedRequest | Refused> {
    if (!verifyStatement(statement)) return refuse("statement proof");
    let plain: Uint8Array;
    try {
      const envelope = decodeRequestEnvelope(statement.data);
      plain = open(
        aeadKey(agree(this.#chatSecret, envelope.ephemeral)),
        envelope.sealed,
      );
    } catch {
      return refuse("not for us");
    }
    let message;
    let messageBytes: Uint8Array;
    let signature: Uint8Array;
    let signer: Uint8Array;
    try {
      const reader = new Reader(plain);
      message = readRequestMessage(reader);
      messageBytes = plain.subarray(0, reader.offset);
      if (reader.u8() !== 0) return refuse("proof kind");
      signature = reader.take(64).slice();
      signer = reader.take(32).slice();
      reader.finish();
    } catch (error) {
      return refuse(
        `request layout: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    if (!REQUEST_ID.test(message.messageId)) return refuse("request id");
    if (
      !sr25519.verify(
        requestProofPayload(messageBytes, this.identity),
        signature,
        signer,
      )
    )
      return refuse("request signature");
    if (equalBytes(message.identityAccount, this.identity))
      return refuse("our own request");
    const chatKey = await this.chatKeyOf(message.identityAccount);
    if (chatKey === null || !usableKey(chatKey))
      return refuse("the sender has no chat key on chain");
    const proof = keyedBlake2b256(
      agree(this.#chatSecret, chatKey),
      identityProofPayload(message.identityAccount, signer),
    );
    if (!equalBytes(proof, message.identityProof))
      return refuse("identity proof");
    if (!usableKey(message.deviceEncryptionKey)) return refuse("device key");
    const id = peerId(message.identityAccount);
    const known = this.peers.get(id);
    if (known !== undefined && known.requestId === message.messageId)
      return {
        peer: id,
        requestId: message.messageId,
        welcome: message.welcome,
        duplicate: true,
      };
    if (known !== undefined && known.state !== "incoming")
      return refuse(
        known.state === "outgoing"
          ? "a request crossed ours, which this prototype does not handle yet"
          : "a new request from an established contact, which this prototype does not handle yet",
      );
    this.peers.set(id, {
      identity: message.identityAccount,
      chatKey,
      state: "incoming",
      requestId: message.messageId,
      devices: [
        {
          statementAccount: signer,
          encryptionKey: message.deviceEncryptionKey,
        },
      ],
    });
    return {
      peer: id,
      requestId: message.messageId,
      welcome: message.welcome,
      duplicate: false,
    };
  }

  /** Decline: forget the pending request from `peer`. Nothing is sent, as on the apps. */
  decline(peer: PeerId): boolean {
    if (this.peers.get(peer)?.state !== "incoming") return false;
    this.peers.delete(peer);
    return true;
  }

  /**
   * After the user's Accept: `DeviceChatAccepted` (kind 20) naming this
   * device, alone on the identity route, encrypted with the identity keys.
   * Unsigned: the host signs it with a claimed notification account, and
   * calls {@link accepted} once it is posted.
   */
  accept(
    peer: PeerId,
    options: { messageId: string; nowMs: number; expiry: bigint },
  ): Statement | Refused {
    const known = this.peers.get(peer);
    if (known?.state !== "incoming") return refuse("no pending request");
    const shared = agree(this.#chatSecret, known.chatKey);
    const topic = sessionTopic(shared, this.identity, known.identity);
    const message = encodeMessage({
      id: options.messageId,
      timestamp: BigInt(options.nowMs),
      content: {
        kind: "DeviceChatAccepted",
        requestId: known.requestId,
        device: {
          statementAccount: this.identity,
          encryptionKey: this.devicePublic,
        },
      },
    });
    return {
      expiry: options.expiry,
      channel: requestChannel(topic),
      topics: [topic],
      data: seal(
        aeadKey(shared),
        encodeStatementData({
          kind: "Request",
          requestId: options.messageId,
          messages: [message],
        }),
      ),
    };
  }

  /** The acceptance of `peer`'s request is posted: the chat is established. */
  accepted(peer: PeerId): boolean {
    const known = this.peers.get(peer);
    if (known?.state !== "incoming") return false;
    known.state = "established";
    return true;
  }

  /** The identity-route topic where `peer` accepts our request, or null when none is pending. */
  acceptanceTopic(peer: PeerId): Uint8Array | null {
    const known = this.peers.get(peer);
    if (known?.state !== "outgoing") return null;
    return sessionTopic(
      agree(this.#chatSecret, known.chatKey),
      known.identity,
      this.identity,
    );
  }

  /** Reads `peer`'s kind 20 for our pending request: alone, for our request id, naming a usable device key. */
  openAcceptance(
    peer: PeerId,
    statement: SignedStatement,
  ): { established: true } | Refused {
    const known = this.peers.get(peer);
    if (known?.state !== "outgoing") return refuse("no pending request");
    if (!verifyStatement(statement)) return refuse("statement proof");
    const shared = agree(this.#chatSecret, known.chatKey);
    const expected = sessionTopic(shared, known.identity, this.identity);
    if (
      statement.topics[0] === undefined ||
      !equalBytes(statement.topics[0], expected)
    )
      return refuse("topic");
    let data;
    try {
      data = decodeStatementData(open(aeadKey(shared), statement.data));
    } catch {
      return refuse("decrypt");
    }
    if (data.kind !== "Request" || data.messages.length !== 1)
      return refuse("not an acceptance");
    const message = decodeMessage(data.messages[0]!);
    if (message.content.kind !== "DeviceChatAccepted")
      return refuse("not an acceptance");
    if (message.content.requestId !== known.requestId)
      return refuse("another request");
    if (!usableKey(message.content.device.encryptionKey))
      return refuse("device key");
    known.devices = [message.content.device];
    known.state = "established";
    return { established: true };
  }

  // ---- The worker's route operations, for an established peer -----------------------

  private established(peer: PeerId): Peer | null {
    const known = this.peers.get(peer);
    return known?.state === "established" && known.devices.length > 0
      ? known
      : null;
  }

  /** Our device-route topic, and one incoming topic for each device of `peer` the host verified. */
  routes(
    peer: PeerId,
  ): { outgoing: Uint8Array; incoming: Uint8Array[] } | Refused {
    const known = this.established(peer);
    if (known === null) return refuse("not an accepted contact");
    return {
      outgoing: sessionTopic(
        agree(this.#deviceSecret, known.chatKey),
        this.identity,
        known.identity,
      ),
      incoming: known.devices.map((device) =>
        sessionTopic(
          agree(this.#chatSecret, device.encryptionKey),
          device.statementAccount,
          this.identity,
        ),
      ),
    };
  }

  /**
   * Seals the worker's frame on the device route, in a multi-device envelope
   * with one entry for each verified device of `peer`. A request frame may
   * carry user content only. Unsigned: the worker signs it with its
   * conversation account.
   */
  sealFrame(peer: PeerId, frame: Frame, expiry: bigint): Statement | Refused {
    const known = this.established(peer);
    if (known === null) return refuse("not an accepted contact");
    if (frame.kind === "Request")
      for (const encoded of frame.messages) {
        const message = decodeMessage(encoded);
        if (!isUserContent(message.content))
          return refuse(`content ${message.content.kind}`);
      }
    const shared = agree(this.#deviceSecret, known.chatKey);
    const topic = sessionTopic(shared, this.identity, known.identity);
    const payloadKey = this.random(32);
    const data = encodeStatementData({
      kind: frame.kind === "Request" ? "MultiRequest" : "MultiResponse",
      payload: seal(payloadKey, encodeFrameBody(frame)),
      devices: known.devices.map((device) => ({
        statementAccount: device.statementAccount,
        encryptedKey: seal(
          aeadKey(agree(this.#deviceSecret, device.encryptionKey)),
          payloadKey,
        ),
      })),
    });
    return {
      expiry,
      channel:
        frame.kind === "Request"
          ? requestChannel(topic)
          : responseChannel(topic),
      topics: [topic],
      data: seal(aeadKey(shared), data),
    };
  }

  /**
   * Opens a statement the worker read on one of `peer`'s incoming topics:
   * the statement proof by any signer, a topic of a verified device, the
   * outer encryption, our envelope entry and a strict layout. Kinds other
   * than user content reach the worker only as stubs with id and timestamp.
   */
  openFrame(peer: PeerId, statement: SignedStatement): WorkerFrame | Refused {
    const known = this.established(peer);
    if (known === null) return refuse("not an accepted contact");
    if (!verifyStatement(statement)) return refuse("statement proof");
    const topic = statement.topics[0];
    const device =
      topic === undefined
        ? undefined
        : known.devices.find((candidate) =>
            equalBytes(
              topic,
              sessionTopic(
                agree(this.#chatSecret, candidate.encryptionKey),
                candidate.statementAccount,
                this.identity,
              ),
            ),
          );
    if (device === undefined) return refuse("topic");
    let frame: Frame;
    try {
      const outer = aeadKey(agree(this.#chatSecret, device.encryptionKey));
      const data = decodeStatementData(open(outer, statement.data));
      if (data.kind === "Request" || data.kind === "Response") frame = data;
      else {
        const entry = data.devices.find((candidate) =>
          equalBytes(candidate.statementAccount, this.identity),
        );
        if (entry === undefined) return refuse("no entry for this device");
        const payloadKey = open(
          aeadKey(agree(this.#deviceSecret, device.encryptionKey)),
          entry.encryptedKey,
        );
        if (payloadKey.length !== 32) return refuse("payload key");
        frame = decodeFrameBody(
          data.kind === "MultiRequest" ? "Request" : "Response",
          open(payloadKey, data.payload),
        );
      }
    } catch {
      return refuse("decrypt or layout");
    }
    if (frame.kind === "Response") return frame;
    return {
      kind: "Request",
      requestId: frame.requestId,
      messages: frame.messages.map((encoded) => {
        const message = decodeMessage(encoded);
        if (isUserContent(message.content)) return message;
        return {
          id: message.id,
          timestamp: message.timestamp,
          content: {
            kind: "Unsupported" as const,
            index:
              message.content.kind === "Unsupported"
                ? message.content.index
                : 20,
          },
        };
      }),
    };
  }

  // ---- Restart ----------------------------------------------------------------------

  /** What this boundary knows of its peers, as JSON. No secret is in it: the keys come from the wallet again. */
  snapshot(): string {
    return JSON.stringify(
      [...this.peers.values()].map((peer) => ({
        identity: bytesToHex(peer.identity),
        chatKey: bytesToHex(peer.chatKey),
        state: peer.state,
        requestId: peer.requestId,
        devices: peer.devices.map((device) => ({
          statementAccount: bytesToHex(device.statementAccount),
          encryptionKey: bytesToHex(device.encryptionKey),
        })),
      })),
    );
  }

  /** Take back what {@link snapshot} wrote. A record that does not read is skipped. */
  restore(snapshot: string | null): void {
    if (snapshot === null) return;
    let records: unknown;
    try {
      records = JSON.parse(snapshot);
    } catch {
      return;
    }
    if (!Array.isArray(records)) return;
    for (const record of records as Record<string, unknown>[]) {
      try {
        const identity = hexToBytes(record.identity as string);
        const state = record.state as PeerState;
        if (!["outgoing", "incoming", "established"].includes(state)) continue;
        this.peers.set(peerId(identity), {
          identity,
          chatKey: hexToBytes(record.chatKey as string),
          state,
          requestId: String(record.requestId),
          devices: (
            record.devices as {
              statementAccount: string;
              encryptionKey: string;
            }[]
          ).map((device) => ({
            statementAccount: hexToBytes(device.statementAccount),
            encryptionKey: hexToBytes(device.encryptionKey),
          })),
        });
      } catch {
        continue;
      }
    }
  }
}
