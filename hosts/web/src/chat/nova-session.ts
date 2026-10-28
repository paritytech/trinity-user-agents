import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import { chainDirectory } from "./nova/directory.js";
import { ChatEngine, type HopService } from "./nova/engine.js";
import type { ChatObserver } from "./nova/events.js";
import { chatRefusal } from "./nova/gate.js";
import { openHopConnection } from "./nova/hop.js";
import { localNovaIdentity } from "./nova/identity.js";
import { ChatKeys, hopSender } from "./nova/keys.js";
import { StatementStore } from "./nova/statement-store.js";
import { chatRecordKey } from "./nova/store.js";
import {
  TurnCredentials,
  turnIdentity,
  type ProofSource,
} from "./nova/turn.js";
import { NovaChatProvider } from "./nova-provider.js";

/**
 * The page's Nova chat for one signed-in session: the session's keys, checked
 * against the core, and the engine built over this page's transports (the
 * People chain's Statement Store, HOP, TURN and the browser's media). The
 * provider (`nova-provider.ts`) only translates that engine for the Chat
 * screen. The Nova chat worker builds the same engine over TrUAPI instead
 * (`src/chat/worker/worker.ts`).
 */

/** What chat needs from the signed-in session. All of it is public except the entropy. */
export interface ChatSession {
  walletId: string;
  entropy: Uint8Array;
  network: string;
  networkSuffix: string;
  /** What the core reports for the session, to check the host derives the same keys. */
  identityAccountId: HexString | undefined;
  chatPublicKey: HexString | undefined;
  /** The username the host found for the session on the People chain, to show when Asset Hub names none. */
  username: string | undefined;
  chain: ChainProvider;
  genesis: { assetHub: HexString; people: HexString };
  /** The network's identity backend, which issues TURN relay credentials, or null. */
  identityBackend: string | null;
  /**
   * Opens a TrUAPI account client as `productId`, for the personhood proof
   * that buys relay credentials. Absent: calls use only the configured ICE
   * servers.
   */
  proofs?: (
    productId: string,
  ) => Promise<{ source: ProofSource; dispose(): void }>;
}

/**
 * Where the ICE servers for calls are configured, as a JSON `RTCIceServer`
 * list. Empty unless set: the apps' TURN credentials come from a backend this
 * host has no access to, so with none set a call connects only where the two
 * devices reach each other directly.
 */
export const iceServersKey = (network: string): string =>
  `truapi-web-signing-host:network:${network}:ice-servers`;

export function iceServers(storage: Storage, network: string): RTCIceServer[] {
  try {
    const parsed: unknown = JSON.parse(
      storage.getItem(iceServersKey(network)) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed.filter(
          (entry): entry is RTCIceServer =>
            typeof entry === "object" &&
            entry !== null &&
            (typeof entry.urls === "string" || Array.isArray(entry.urls)),
        )
      : [];
  } catch {
    return [];
  }
}

/**
 * Where the HOP nodes of a network are configured. No network this host
 * knows publishes them yet, so the list is empty unless set here, as a JSON
 * array of `wss://` URLs, for a development test.
 */
export const hopNodesKey = (network: string): string =>
  `truapi-web-signing-host:network:${network}:hop-nodes`;

/** The configured HOP nodes: `wss://` URLs only, anything else dropped. */
export function hopNodes(storage: Storage, network: string): string[] {
  try {
    const parsed: unknown = JSON.parse(
      storage.getItem(hopNodesKey(network)) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed.filter(
          (url): url is string =>
            typeof url === "string" && /^wss:\/\/[^\s]+$/.test(url),
        )
      : [];
  } catch {
    return [];
  }
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/**
 * Derive the session's chat keys and check them against what the core
 * reports. Returns why chat stays off instead, when it does.
 */
export function sessionChatKeys(
  session: Pick<
    ChatSession,
    "entropy" | "networkSuffix" | "identityAccountId" | "chatPublicKey"
  >,
): ChatKeys | string {
  const refusal = chatRefusal(session.entropy);
  if (refusal !== null) return refusal;
  let keys: ChatKeys;
  try {
    keys = ChatKeys.fromEntropy(session.entropy, session.networkSuffix);
  } catch (error) {
    return `Chat keys could not be derived: ${errorText(error)}`;
  }
  if (
    session.identityAccountId === undefined ||
    session.chatPublicKey === undefined
  )
    return "The core did not report this session's identity account and chat key, so chat cannot check its keys.";
  const mismatch = [
    session.identityAccountId !== bytesToHex(keys.identity)
      ? "identity account"
      : null,
    session.chatPublicKey !== bytesToHex(keys.chatPublic) ? "chat key" : null,
  ].filter((part) => part !== null);
  if (mismatch.length > 0)
    return `The host derived a different ${mismatch.join(" and ")} than the core. Chat stays off.`;
  return keys;
}

/**
 * Derive and check the session's keys, and start the engine. Returns why
 * chat stays off instead, when it does.
 */
export function startNovaChat(
  session: ChatSession,
  deps: { storage: Storage; diagnostics: ChatObserver },
): NovaChatProvider | string {
  const keys = sessionChatKeys(session);
  if (typeof keys === "string") return keys;
  const hop: HopService = {
    nodes: () => hopNodes(deps.storage, session.network),
    connect: openHopConnection,
    sender: hopSender(session.entropy),
  };
  const engine = new ChatEngine({
    hop,
    identity: localNovaIdentity(keys),
    openStore: async () =>
      new StatementStore(
        await session.chain.connect(hexToBytes(session.genesis.people)),
        deps.diagnostics.payloads.connectionTap(),
      ),
    directory: chainDirectory(session.chain, session.genesis),
    storage: deps.storage,
    recordKey: chatRecordKey(session.network, session.walletId),
    writes: () => true,
    diagnostics: deps.diagnostics.record,
    payloads: deps.diagnostics.payloads,
  });
  const proofs = session.proofs;
  const identity = turnIdentity(session.networkSuffix);
  const turn =
    proofs === undefined
      ? null
      : new TurnCredentials({
          identity,
          issuer: session.identityBackend,
          proofs: () => proofs(identity.caller),
          fetch: (url, init) => fetch(url, init),
        });
  const provider = new NovaChatProvider(
    engine,
    session.username,
    {
      media: (video) =>
        navigator.mediaDevices.getUserMedia({ audio: true, video }),
      rtc: { iceServers: iceServers(deps.storage, session.network) },
      ...(turn === null ? {} : { relays: () => turn.iceServers() }),
      now: Date.now,
    },
    turn,
  );
  void engine.start();
  return provider;
}
