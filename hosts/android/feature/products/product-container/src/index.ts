import { createContainer } from '@novasamatech/host-container';
import type { Provider, Subscription } from '@novasamatech/host-api';
import { RequestCredentialsErr, CreateProofErr, GetAliasErr, SignVrfErr, RegisterRingVrfKeyErr, ListRingVrfKeysErr, RingVrfSignErr, ChatMessagePostingErr, NavigateToErr, StorageErr, SigningErr, PreimageSubmitErr, StatementProofErr, GenericError, DeriveEntropyErr, PaymentRequestErr, PaymentStatusErr, PaymentTopUpErr, PaymentTopUpStatusErr, ResourceAllocationErr, CreateTransactionErr, CustomRendererNode, PushNotificationError, GetUserIdErr, WorkerErr, toHex, fromHex } from '@novasamatech/host-api';
import { createWsJsonRpcProvider } from '@novasamatech/host-substrate-chain-connection';

type PausableJsonRpcProvider = ReturnType<typeof createWsJsonRpcProvider>;
import { createNativeTransport } from './native-transport';

type WireDerivationIndex = { tag: 'Index'; value: number } | { tag: 'Raw'; value: Uint8Array };
import { WebRtcManager } from './webrtc-manager';

// =============================================================================
// Isolation: Capture private refs BEFORE locking down globals.
// The container IIFE closure keeps these inaccessible to product scripts.
// =============================================================================

// Captured before sandbox lockdown freezes window.WebSocket below.
const CapturedWebSocket = globalThis.WebSocket;

// Captured before the permission-gated replacement is installed below.
const CapturedRTCPeerConnection = window.RTCPeerConnection;

// =============================================================================
// Isolation: Lock down globals so product scripts cannot access platform APIs.
// =============================================================================

function freezeAndDelete(obj: any, prop: string) {
  try {
    Object.defineProperty(obj, prop, {
      value: undefined,
      writable: false,
      configurable: false,
    });
  } catch {
    // Property may already be non-configurable; try delete as fallback
    try { delete obj[prop]; } catch { /* best effort */ }
  }
}

function freezeValue(obj: any, prop: string, value: any) {
  try {
    Object.defineProperty(obj, prop, {
      value,
      writable: false,
      configurable: false,
    });
  } catch { /* best effort */ }
}

// --- Network: intercept with error (future: permission-gated) ---

// TODO commented out for now since we only have trusted products
// freezeValue(window, 'XMLHttpRequest', function XMLHttpRequest() {
//   throw new TypeError('Network access is not allowed');
// });
//
// freezeValue(window, 'WebSocket', function WebSocket() {
//   throw new TypeError('Network access is not allowed');
// });

// --- Network: delete (no future permission path) ---
// freezeAndDelete(window, 'EventSource');

freezeValue(navigator, 'sendBeacon', () => false);

// --- Storage ---
freezeAndDelete(window, 'indexedDB');
freezeAndDelete(window, 'caches');

// document.cookie — redefine as no-op getter/setter
try {
  Object.defineProperty(document, 'cookie', {
    get: () => '',
    set: () => {},
    configurable: false,
  });
} catch { /* best effort */ }

// --- Workers ---
freezeAndDelete(window, 'SharedWorker');

if (navigator.serviceWorker) {
  try {
    Object.defineProperty(navigator, 'serviceWorker', {
      value: Object.freeze({ register: () => { throw new Error('ServiceWorker is not available'); } }),
      writable: false,
      configurable: false,
    });
  } catch { /* best effort */ }
}

// --- DOM: block iframe creation ---
// TODO: disabled for now since we need iframes to work. We need to figure out reasonable iframe restrictions by the time trustees products appear
// const _createElement = document.createElement.bind(document);
// freezeValue(document, 'createElement', (tagName: string, options?: ElementCreationOptions) => {
//   if (tagName.toLowerCase() === 'iframe') {
//     throw new Error('iframe creation is not allowed');
//   }
//   return _createElement(tagName, options);
// });

(window as any).__HOST_WEBVIEW_MARK__ = true;

const { callNative, subscribeNative } = createNativeTransport((message) => {
  const json = JSON.stringify(message);
  (window as any).Android.call('__container__', json);
});

if (CapturedRTCPeerConnection) {
  const webRtcManager = new WebRtcManager(
    CapturedRTCPeerConnection,
    () => callNative('allowWebRtcAccess', {}).then((allowed: unknown) => allowed === true),
  );
  freezeValue(window, 'RTCPeerConnection', webRtcManager.connectionClass);
} else {
  freezeAndDelete(window, 'RTCPeerConnection');
}

const { port1, port2 } = new MessageChannel();

(window as any).__HOST_API_PORT__ = port1;

const subscribers = new Set<(message: Uint8Array) => void>();

port2.onmessage = (event: MessageEvent) => {
  for (const subscriber of subscribers) {
    subscriber(event.data);
  }
};

const containerProvider: Provider = {
  logger: console,
  isCorrectEnvironment() {
    return true;
  },
  postMessage(message: Uint8Array) {
    port2.postMessage(message, [message.buffer]);
  },
  subscribe(callback: (message: Uint8Array) => void) {
    subscribers.add(callback);
    return () => { subscribers.delete(callback); };
  },
  dispose() {
    subscribers.clear();
  },
};

const container = createContainer(containerProvider);

/**
 * RFC-0022: native takes the account selector in its tolerant JSON form — a number for a plain
 * index, a 0x-hex string for a raw 32-byte one. Native re-expands `Left(n)` to `index_bytes(n)`.
 *
 * Handler params arrive already decoded into the tagged wire form, so this reads the tag rather
 * than normalizing again — `derivationIndexOf` runs the other direction, selector to wire.
 */
function toNativeDerivationIndex(index: WireDerivationIndex): number | string {
  return index.tag === 'Index' ? index.value : toHex(index.value);
}

// Native reads a ProductAccountId as the 2-element tuple `[productId, number | hex]`; its tuple
// adapter rejects the tagged wire enum, so the selector must be flattened before the call. RFC-0024
// key handles share this shape while naming a slot in the ring VRF tree, and use it unchanged.
function toNativeProductAccountId(account: readonly [string, WireDerivationIndex]): [string, number | string] {
  const [productId, derivationIndex] = account;
  return [productId, toNativeDerivationIndex(derivationIndex)];
}

container.handleAccountGet((params, { ok, err }) => {
  const promise: Promise<any> = callNative('accountGet', {
    account: toNativeProductAccountId(params),
  }).then(
    (result) => ok({
      publicKey: fromHex(result.publicKey),
      name: result.name ?? undefined,
    }),
    (e) => err(new RequestCredentialsErr.Unknown({ reason: String(e) })),
  );
  return promise;
});

// RFC-0004 (amended by RFC-0022): context is a [productId, suffix] tuple where the suffix is the
// same selector an account carries; ring is { chainId, junctions }. Native expects bytes as hex.
function toNativeContext(context: [string, WireDerivationIndex]) {
  const [productId, suffix] = context;
  return { productId, suffix: toNativeDerivationIndex(suffix) };
}

function toNativeRing(ring: { chainId: string; junctions: Array<{ tag: string; value: any }> }) {
  return {
    chainId: ring.chainId,
    junctions: ring.junctions.map((junction) => ({
      tag: junction.tag,
      value: junction.tag === 'CollectionId' ? toHex(junction.value) : junction.value,
    })),
  };
}

container.handleAccountGetAlias((params, { ok, err }) => {
  const [keyHandle, context, ring] = params;
  return callNative('accountGetAlias', {
    keyHandle: toNativeProductAccountId(keyHandle),
    context: toNativeContext(context),
    ring: toNativeRing(ring),
  }).then(
    (result) => ok({
      context: fromHex(result.context),
      alias: fromHex(result.alias),
    }),
    (e) => {
      switch (e?.code) {
        case 'RingNotFound':
          return err(new GetAliasErr.RingNotFound());
        case 'NotMember':
          return err(new GetAliasErr.NotMember());
        case 'KeyNotRegistered':
          return err(new GetAliasErr.KeyNotRegistered());
        case 'KeyNotInRing':
          return err(new GetAliasErr.KeyNotInRing());
        case 'Rejected':
          return err(new GetAliasErr.Rejected());
        default:
          return err(new GetAliasErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

container.handleAccountCreateProof((params, { ok, err }) => {
  const [keyHandle, context, ring, message] = params;
  return callNative('accountCreateProof', {
    keyHandle: toNativeProductAccountId(keyHandle),
    context: toNativeContext(context),
    ring: toNativeRing(ring),
    message: toHex(message),
  }).then(
    (result) => ok({
      proof: fromHex(result.proof),
      contextualAlias: {
        context: fromHex(result.contextualAlias.context),
        alias: fromHex(result.contextualAlias.alias),
      },
      ringIndex: result.ringIndex,
      ringRevision: result.ringRevision,
    }),
    (e) => {
      switch (e?.code) {
        case 'RingNotFound':
          return err(new CreateProofErr.RingNotFound());
        case 'NotMember':
          return err(new CreateProofErr.NotMember());
        case 'KeyNotRegistered':
          return err(new CreateProofErr.KeyNotRegistered());
        case 'KeyNotInRing':
          return err(new CreateProofErr.KeyNotInRing());
        case 'NotAllowlisted':
          return err(new CreateProofErr.NotAllowlisted());
        case 'Rejected':
          return err(new CreateProofErr.Rejected());
        default:
          return err(new CreateProofErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

// --- Ring VRF key registry (RFC-0024) ---

function fromNativeRing(ring: { chainId: string; junctions: Array<{ tag: string; value: string }> }) {
  return {
    chainId: ring.chainId,
    junctions: ring.junctions.map((junction) =>
      junction.tag === 'CollectionId'
        ? { tag: 'CollectionId', value: fromHex(junction.value) }
        : { tag: 'PalletInstance', value: Number(junction.value) },
    ),
  };
}

/** Inverse of {@link toNativeDerivationIndex}: the wire form is a tagged enum, not a bare value. */
function fromNativeDerivationIndex(index: number | string): WireDerivationIndex {
  return typeof index === 'number'
    ? { tag: 'Index', value: index }
    : { tag: 'Raw', value: fromHex(index) };
}

function fromNativeRegistryEntry(entry: any) {
  return {
    handle: [entry.handle[0], fromNativeDerivationIndex(entry.handle[1])] as const,
    rings: entry.rings.map(fromNativeRing),
    publicKey: entry.publicKey ? fromHex(entry.publicKey) : undefined,
  };
}

container.handleAccountRegisterRingVrfKey((params, { ok, err }) => {
  const [index, ring] = params;
  return callNative('registerRingVrfKey', {
    index: toNativeDerivationIndex(index),
    ring: toNativeRing(ring),
  }).then(
    (result) => ok(fromHex(result.publicKey)),
    (e) => {
      switch (e?.code) {
        case 'NotConnected':
          return err(new RegisterRingVrfKeyErr.NotConnected());
        case 'RingNotFound':
          return err(new RegisterRingVrfKeyErr.RingNotFound());
        case 'Rejected':
          return err(new RegisterRingVrfKeyErr.Rejected());
        default:
          return err(new RegisterRingVrfKeyErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

container.handleAccountListRingVrfKeys((params, { ok, err }) => {
  const [owner, disclosure] = params;
  return callNative('listRingVrfKeys', { owner, disclosure: disclosure ?? 'Anonymized' }).then(
    (result: any[]) => ok(result.map(fromNativeRegistryEntry)),
    (e) => {
      switch (e?.code) {
        case 'NotConnected':
          return err(new ListRingVrfKeysErr.NotConnected());
        case 'Rejected':
          return err(new ListRingVrfKeysErr.Rejected());
        default:
          return err(new ListRingVrfKeysErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

container.handleAccountRingVrfSign((params, { ok, err }) => {
  const [keyHandle, message] = params;
  return callNative('ringVrfSign', {
    keyHandle: toNativeProductAccountId(keyHandle),
    message: toHex(message),
  }).then(
    (result) => ok(fromHex(result.signature)),
    (e) => {
      switch (e?.code) {
        case 'NotConnected':
          return err(new RingVrfSignErr.NotConnected());
        case 'KeyNotRegistered':
          return err(new RingVrfSignErr.KeyNotRegistered());
        case 'NotAllowlisted':
          return err(new RingVrfSignErr.NotAllowlisted());
        case 'Rejected':
          return err(new RingVrfSignErr.Rejected());
        default:
          return err(new RingVrfSignErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

// --- Sign VRF (RFC-0023) ---

container.handleAccountSignVrf((params, { ok, err }) => {
  return callNative('accountSignVrf', {
    account: toNativeProductAccountId(params.account),
    label: toHex(params.transcriptLabel),
    items: params.items.map((item) => ({
      label: toHex(item.label),
      value: toHex(item.value),
    })),
  }).then(
    (result) => ok({
      preOutput: fromHex(result.preOutput),
      proof: fromHex(result.proof),
    }),
    (e) => {
      switch (e?.code) {
        case 'NotConnected':
          return err(new SignVrfErr.NotConnected());
        case 'Rejected':
          return err(new SignVrfErr.Rejected());
        default:
          return err(new SignVrfErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

// --- Get User ID ---

container.handleGetUserId(async (_params, { ok, err }) => {
  try {
    const result = await callNative('getUserId', {});
    return ok({ primaryUsername: result.primaryUsername });
  } catch (e) {
    const reason = String(e instanceof Error ? e.message : e);
    if (reason.includes('NotConnected')) {
      return err(new GetUserIdErr.NotConnected());
    }
    if (reason.includes('Permission denied')) {
      return err(new GetUserIdErr.PermissionDenied());
    }
    return err(new GetUserIdErr.Unknown({ reason }));
  }
});

container.handleFeatureSupported((params, { ok }) => {
  switch (params.tag) {
    case 'Chain':
      return callNative('chainSupported', { genesisHash: params.value })
        .then((supported: boolean) => ok(supported))
        .catch(() => ok(false));
    default:
      return ok(false);
  }
});

const activeProviders = new Set<PausableJsonRpcProvider>();

container.handleChainConnection((genesisHash) => {
  return (onMessage) => {
    let inner: { send(msg: string): void; disconnect(): void } | null = null;
    let provider: PausableJsonRpcProvider | null = null;
    const buffer: string[] = [];

    callNative('chainNodes', { genesisHash }).then((urls: string[]) => {
      provider = createWsJsonRpcProvider({
        endpoints: urls,
        websocketClass: CapturedWebSocket
      });
      activeProviders.add(provider);
      inner = provider(onMessage) as unknown as { send(msg: string): void; disconnect(): void };
      for (const msg of buffer) inner.send(msg);
      buffer.length = 0;
    });

    return {
      send(message: string) {
        if (inner) inner.send(message);
        else buffer.push(message);
      },
      disconnect() {
        inner?.disconnect();
        if (provider) {
          activeProviders.delete(provider);
          provider = null;
        }
      },
    };
  };
});

(window as any).__pauseConnections__ = () => {
  for (const provider of activeProviders) {
    provider.pause();
  }
};

(window as any).__resumeConnections__ = () => {
  for (const provider of activeProviders) {
    provider.resume();
  }
};

// --- Entropy Derivation ---

container.handleDeriveEntropy(async (key, { ok, err }) => {
  try {
    const result = await callNative('deriveEntropy', { key: toHex(key) });
    return ok(fromHex(result.entropy));
  } catch (e) {
    return err(new DeriveEntropyErr.Unknown({ reason: String(e) }));
  }
});

// --- Signing ---

type RawSignPayload = { tag: 'Bytes'; value: Uint8Array } | { tag: 'Payload'; value: string };
type CreateTxPayload = {
  // The `GenesisHash` codec decodes to a 0x-string, unlike the `Bytes()` fields below.
  genesisHash: string;
  callData: Uint8Array;
  extensions: { id: string; additionalSigned: Uint8Array; extra: Uint8Array }[];
  txExtVersion: number;
};

const rawSignNativeData = (payload: RawSignPayload) =>
  payload.tag === 'Bytes' ? { data: toHex(payload.value) } : { payload: payload.value };

// `signer` is the product-account tuple for product signing, or a hex account id for legacy signing.
const createTxNativeParams = (signer: unknown, payload: CreateTxPayload) => ({
  signer,
  genesisHash: payload.genesisHash,
  callData: toHex(payload.callData),
  extensions: payload.extensions.map((e) => ({
    id: e.id,
    implicit: toHex(e.additionalSigned),
    explicit: toHex(e.extra),
  })),
  txExtVersion: payload.txExtVersion,
});

container.handleSignPayload(async ({ account, payload }, { ok, err }) => {
  try {
    const result = await callNative('signPayload', { account: toNativeProductAccountId(account), ...payload });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch (e) {
    return err(new SigningErr.Rejected());
  }
});

container.handleSignRaw(async ({ account, payload }, { ok, err }) => {
  try {
    const result = await callNative('signRaw', { account: toNativeProductAccountId(account), ...rawSignNativeData(payload) });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch (e) {
    return err(new SigningErr.Rejected());
  }
});

// --- Create Transaction (RFC-0020) ---

container.handleCreateTransaction(async (payload, { ok, err }) => {
  try {
    const result = await callNative('createTransaction', createTxNativeParams(toNativeProductAccountId(payload.signer), payload));
    return ok(fromHex(result.signedTx));
  } catch (e) {
    return err(new CreateTransactionErr.Unknown({ reason: String(e) }));
  }
});

// --- Legacy account signing ---
// These sign with a plain account id (e.g. the user's identity account) instead of a product
// account. The native side reverse-resolves the account before showing the signing screen.

container.handleSignRawWithLegacyAccount(async ({ signer, payload }, { ok, err }) => {
  try {
    const result = await callNative('signRawWithLegacyAccount', { account: signer, ...rawSignNativeData(payload) });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch (e) {
    return err(new SigningErr.Rejected());
  }
});

container.handleCreateTransactionWithLegacyAccount(async (payload, { ok, err }) => {
  try {
    const result = await callNative('createTransactionWithLegacyAccount', createTxNativeParams(toHex(payload.signer), payload));
    return ok(fromHex(result.signedTx));
  } catch (e) {
    return err(new CreateTransactionErr.Unknown({ reason: String(e) }));
  }
});

// --- Account Connection Status ---

container.handleAccountConnectionStatusSubscribe((_params, send, _interrupt) => {
  send('connected');
  return () => {};
});

// --- Theme ---

container.handleThemeSubscribe((_params, send, interrupt) => {
  return subscribeNative(
    'themeSubscribe',
    {},
    (payload: { name: { tag: 'Custom' | 'Default'; value?: string }; variant: 'Light' | 'Dark' }) => {
      const name =
        payload.name.tag === 'Custom'
          ? { tag: 'Custom' as const, value: payload.name.value ?? '' }
          : { tag: 'Default' as const, value: undefined };
      send({ name, variant: payload.variant });
    },
    () => interrupt(),
  );
});

// --- Request Login ---

container.handleRequestLogin((_reason, { ok }) => ok('alreadyConnected'));

// --- Device Permission ---

container.handleDevicePermission(async (capability, { ok, err }) => {
  console.log(`handleDevicePermission called with: ${capability}`);
  try {
    const result = await callNative('devicePermission', { capability });
    console.log(`handleDevicePermission native result: ${result} (type: ${typeof result})`);
    return ok(result);
  } catch (e) {
    console.log(`handleDevicePermission error: ${e}`);
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Remote Permission ---

container.handlePermission(async (requests, { ok, err }) => {
  try {
    const payload = (Array.isArray(requests) ? requests : [requests]).map((r: any) => ({
      tag: r.tag,
      value: r.tag === 'Remote' ? r.value : undefined,
    }));
    const result = await callNative('remotePermission', payload);
    return ok(result);
  } catch (e) {
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Push Notification ---

container.handlePushNotification(async (params, { ok, err }) => {
  try {
    const scheduledAt = params.scheduledAt !== undefined ? Number(params.scheduledAt) : undefined;
    const id: number = await callNative('pushNotification', {
      text: params.text,
      deeplink: params.deeplink,
      scheduledAtMs: scheduledAt,
    });
    return ok(id);
  } catch (e) {
    const reason = String(e);
    if (reason.includes('Schedule limit reached')) {
      return err(new PushNotificationError.ScheduleLimitReached());
    }
    return err(new PushNotificationError.Unknown({ reason }));
  }
});

container.handlePushNotificationCancel(async (identifier, { ok, err }) => {
  try {
    await callNative('cancelPushNotification', identifier);
    return ok(undefined);
  } catch (e) {
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Non-Product Accounts ---

container.handleGetLegacyAccounts((_params, { ok, err }) => {
  return callNative('getLegacyAccounts', {}).then(
    (result: { publicKey: string; name?: string }[]) => ok(result.map((account) => ({
      publicKey: fromHex(account.publicKey),
      name: account.name ?? undefined,
    }))),
    (e) => err(new RequestCredentialsErr.Unknown({ reason: String(e) })),
  );
});

// --- Preimage Lookup ---

container.handlePreimageLookupSubscribe((hashHex, send, _interrupt) => {
  callNative('preimageLookup', { hash: hashHex }).then(
    (result) => send(result.data ? fromHex(result.data) : null),
    () => send(null),
  );
  return () => {};
});

// --- Preimage Submit ---

container.handlePreimageSubmit(async (data, { ok, err }) => {
  try {
    const result = await callNative('preimageSubmit', { data: toHex(data) });
    return ok(result.hash);
  } catch (e) {
    return err(new PreimageSubmitErr.Unknown({ reason: String(e) }));
  }
});

// --- Statement Store: shared validation ---

const STATEMENT_TOPIC_BYTES = 32;

// Statement-store topics are fixed-size 32-byte hashes; reject malformed input before it reaches native.
function validateStatementTopics(topics: Uint8Array[]): string | null {
  const invalid = topics.find((topic) => topic.length !== STATEMENT_TOPIC_BYTES);
  if (invalid === undefined) return null;

  const asUtf8 = new TextDecoder().decode(invalid);
  return `Statement topic must be ${STATEMENT_TOPIC_BYTES} bytes, got ${invalid.length}: ${toHex(invalid)} ("${asUtf8}")`;
}

// --- Statement Store Create Proof ---

container.handleStatementStoreCreateProof(async ([account, statement], { ok, err }) => {
  const topicError = validateStatementTopics(statement.topics);
  if (topicError) return err(new StatementProofErr.Unknown({ reason: topicError }));

  try {
    const result = await callNative('createStatementProof', {
      account: toNativeProductAccountId(account),
      statement: {
        channel: statement.channel ? toHex(statement.channel) : undefined,
        expiry: statement.expiry?.toString() ?? undefined,
        topics: statement.topics.map((t) => toHex(t)),
        data: statement.data ? toHex(statement.data) : undefined,
      },
    });
    return ok({
      tag: result.tag as 'Sr25519',
      value: { signature: fromHex(result.signature), signer: fromHex(result.signer) },
    });
  } catch (e) {
    return err(new StatementProofErr.UnableToSign());
  }
});

// --- Statement Store Create Proof Authorized (RFC-0010) ---

container.handleStatementStoreCreateProofAuthorized(async (statement, { ok, err }) => {
  const topicError = validateStatementTopics(statement.topics);
  if (topicError) return err(new StatementProofErr.Unknown({ reason: topicError }));

  try {
    const result = await callNative('createStatementProofAuthorized', {
      channel: statement.channel ? toHex(statement.channel) : undefined,
      expiry: statement.expiry?.toString() ?? undefined,
      topics: statement.topics.map((t) => toHex(t)),
      data: statement.data ? toHex(statement.data) : undefined,
    });
    return ok({
      tag: result.tag as 'Sr25519',
      value: { signature: fromHex(result.signature), signer: fromHex(result.signer) },
    });
  } catch (e) {
    return err(new StatementProofErr.UnableToSign());
  }
});

// --- Resource Allocation (RFC-0010) ---

container.handleRequestResourceAllocation(async (resources, { ok, err }) => {
  try {
    const dtos = resources.map((r) => {
      switch (r.tag) {
        case 'SmartContractAllowance':
          return { kind: r.tag, dest: toNativeDerivationIndex(r.value) };
        case 'StatementStoreAllowance':
        case 'BulletinAllowance':
        case 'AutoSigning':
          return { kind: r.tag };
      }
    });
    const result = await callNative('hostRequestResourceAllocation', { resources: dtos });
    const outcomes = (result.outcomes as { kind: string }[]).map((o) => ({
      tag: o.kind as 'Allocated' | 'Rejected' | 'NotAvailable',
      value: undefined,
    }));
    return ok(outcomes);
  } catch (e) {
    return err(new ResourceAllocationErr.Unknown({ reason: String(e) }));
  }
});

// --- Statement Store Submit ---

container.handleStatementStoreSubmit(async (statement, { ok, err }) => {
  const topicError = validateStatementTopics(statement.topics);
  if (topicError) return err(new GenericError({ reason: topicError }));

  try {
    await callNative('statementStoreSubmit', {
      proof: {
        tag: statement.proof.tag,
        signature: toHex(statement.proof.value.signature),
        signer: toHex(statement.proof.value.signer),
      },
      channel: statement.channel ? toHex(statement.channel) : undefined,
      expiry: statement.expiry?.toString() ?? undefined,
      topics: statement.topics.map((t) => toHex(t)),
      data: statement.data ? toHex(statement.data) : undefined,
    });
    return ok(undefined);
  } catch (e) {
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Statement Store Subscribe ---

container.handleStatementStoreSubscribe((filter, send, interrupt) => {
  const topicError = validateStatementTopics(filter.value);
  if (topicError) {
    console.error(topicError);
    interrupt();
    return () => {};
  }

  const hexTopics = filter.value.map((t) => toHex(t));
  const nativeFilter = filter.tag === 'MatchAll' ? { matchAll: hexTopics } : { matchAny: hexTopics };
  const unsub = subscribeNative('statementStoreSubscribe', { filter: nativeFilter }, (page: { statements: { proof: { tag: string; signature: string; signer: string }; channel?: string; expiry: string; topics: string[]; data?: string }[]; isComplete: boolean }) => {
    send({
      statements: page.statements.map((s) => ({
        proof: { tag: s.proof.tag as 'Sr25519', value: { signature: fromHex(s.proof.signature), signer: fromHex(s.proof.signer) } },
        decryptionKey: undefined,
        expiry: s.expiry ? BigInt(s.expiry) : undefined,
        channel: s.channel ? fromHex(s.channel) : undefined,
        topics: s.topics.map((t: string) => fromHex(t)),
        data: s.data ? fromHex(s.data) : undefined,
      })),
      isComplete: page.isComplete,
    });
  });
  return unsub;
});

// --- Chat: Create Room ---

container.handleChatCreateRoom(async (params, { ok, err }) => {
  try {
    const result = await callNative('chatCreateRoom', params);
    return ok({ status: result.status });
  } catch (e) {
    return err({ reason: String(e) });
  }
});

// --- Chat: List Subscribe ---

container.handleChatListSubscribe((_params, send, _interrupt) => {
  const unsub = subscribeNative('chatListSubscribe', {}, (rooms) => {
    send(rooms);
  });
  return unsub;
});

// --- Chat: Post Message ---

container.handleChatPostMessage(async (params, { ok, err }) => {
  const { payload } = params;

  try {
    switch (payload.tag) {
      case 'Text': {
        const result = await callNative('chatSendTextMessage', { text: payload.value, chatId: params.roomId });
        return ok({ messageId: result.messageId });
      }
      case 'Custom': {
        const result = await callNative('chatSendCustomMessage', {
          messageType: payload.value.messageType,
          payloadHex: toHex(payload.value.payload),
          chatId: params.roomId
        });
        return ok({ messageId: result.messageId });
      }
      default:
        return err(new ChatMessagePostingErr.Unknown({
          reason: `Unsupported message type: ${(payload as any).tag}`,
        }));
    }
  } catch (e) {
    return err(new ChatMessagePostingErr.Unknown({ reason: String(e) }));
  }
});

// --- Chat: Custom Message Rendering ---

const renderSubscriptions = new Map<string, Subscription>();

(window as any).renderMessage = (messageType: string, payloadHex: string, messageId: string) => {
  renderSubscriptions.get(messageId)?.unsubscribe();

  const payload = fromHex(payloadHex);

  const subscription = container.renderChatCustomMessage({ messageId, messageType, payload }, (node) => {
    const scaleHex = toHex(CustomRendererNode.enc(node));
    callNative('chatRenderWidget', { messageId, scaleHex });
  });

  renderSubscriptions.set(messageId, subscription);
};

// --- Chat: Action Subscribe ---

type ChatActionSend = (action: {
  roomId: string;
  peer: string;
  payload:
    | { tag: 'MessagePosted'; value: { tag: 'Text'; value: string } }
    | { tag: 'ActionTriggered'; value: { messageId: string; actionId: string; payload?: Uint8Array } };
}) => void;

let chatActionSend: ChatActionSend | null = null;

container.handleChatActionSubscribe((_params, send, _interrupt) => {
    console.log("handleChatActionSubscribe called")
  chatActionSend = send;
  return () => { chatActionSend = null; };
});

(window as any).dispatchChatAction = (
  roomId: string,
  messageId: string,
  actionId: string,
  payloadHex?: string,
) => {
  console.log(`Received dispatchChatAction: roomId=${roomId}, messageId=${messageId}, actionId=${actionId}, payload=${payloadHex}`)

  chatActionSend?.({
    roomId,
    peer: 'native',
    payload: {
      tag: 'ActionTriggered',
      value: {
        messageId,
        actionId,
        payload: payloadHex ? fromHex(payloadHex) : undefined,
      },
    },
  });
};

(window as any).dispatchUserMessage = (roomId: string, text: string) => {
    console.log(`Received dispatchUserMessage: roomId=${roomId}, text=${text}`)

  chatActionSend?.({
    roomId,
    peer: 'native',
    payload: {
      tag: 'MessagePosted',
      value: {
        tag: 'Text',
        value: text,
      },
    },
  });
};

// --- Payment Balance ---

container.handlePaymentBalanceSubscribe((_params, send, interrupt) => {
  return subscribeNative(
    'paymentBalanceSubscribe',
    {},
    (payload: { available: string }) => {
      send({ available: BigInt(payload.available) });
    },
    () => interrupt(),
  );
});

// --- Payment Request (RFC-0006) ---

container.handlePaymentRequest(async (params, { ok, err }) => {
  try {
    await callNative('paymentRequest', {
      amount: params.amount.toString(),
      destinationHex: toHex(params.destination),
      idHex: toHex(params.id),
    });
    return ok(undefined);
  } catch (e) {
    switch ((e as { code?: string })?.code) {
      case 'AlreadyExists':
        return err(new PaymentRequestErr.AlreadyExists());
      case 'Rejected':
        return err(new PaymentRequestErr.Rejected());
      case 'InsufficientBalance':
        return err(new PaymentRequestErr.InsufficientBalance());
      default:
        return err(new PaymentRequestErr.Unknown({ reason: String((e as Error)?.message ?? e) }));
    }
  }
});

container.handlePaymentTopUp(async (params, { ok, err }) => {
  try {
    const nativeParams: Record<string, unknown> = {
      id: toHex(params.id),
      amount: params.amount.toString(),
      sourceTag: params.source.tag,
    };
    if (params.source.tag === 'ProductAccount') {
      nativeParams.sourceDerivationIndex = toNativeDerivationIndex(params.source.value);
    } else if (params.source.tag === 'Coins') {
      nativeParams.sourceKeyListHex = params.source.value.map((key) => toHex(key));
    } else {
      nativeParams.sourceKeyHex = toHex(params.source.value);
    }
    await callNative('paymentTopUp', nativeParams);
    return ok();
  } catch (e) {
    switch ((e as { code?: string })?.code) {
      case 'InvalidSource':
        return err(new PaymentTopUpErr.InvalidSource());
      case 'AlreadyExists':
        return err(new PaymentTopUpErr.AlreadyExists());
      case 'SourceBusy':
        return err(new PaymentTopUpErr.SourceBusy());
      default:
        return err(new PaymentTopUpErr.Unknown({ reason: String((e as Error)?.message ?? e) }));
    }
  }
});

container.handlePaymentTopUpStatusSubscribe((id, send, interrupt) => {
  return subscribeNative(
    'paymentTopUpStatusSubscribe',
    { id: toHex(id) },
    (payload: { tag: string; finalized?: boolean; actualClaimed?: string }) => {
      switch (payload.tag) {
        case 'Claimed':
          return send({ tag: 'Claimed', value: { finalized: payload.finalized ?? false } });
        case 'ClaimedPartially':
          return send({
            tag: 'ClaimedPartially',
            value: { actualClaimed: BigInt(payload.actualClaimed ?? '0') },
          });
        case 'Claiming':
          return send({ tag: 'Claiming', value: undefined });
        case 'NotClaimed':
          return send({ tag: 'NotClaimed', value: undefined });
        default:
          return send({ tag: 'Detecting', value: undefined });
      }
    },
    (e) => {
      // Deferred: an interrupt raised synchronously from this body outruns the product-side
      // subscription bookkeeping and is dropped, so an unknown id would look like silence.
      const failure =
        (e as { code?: string })?.code === 'NotFound'
          ? new PaymentTopUpStatusErr.NotFound()
          : new PaymentTopUpStatusErr.Unknown({ reason: String((e as Error)?.message ?? e) });

      queueMicrotask(() => interrupt(failure));
    },
  );
});

container.handlePaymentStatusSubscribe((id, send, interrupt) => {
  return subscribeNative(
    'paymentStatusSubscribe',
    { idHex: toHex(id) },
    (payload: { tag: string; value: string | null }) => {
      switch (payload.tag) {
        case 'Completed':
          return send({ tag: 'Completed', value: undefined });
        case 'Failed':
          return send({ tag: 'Failed', value: payload.value ?? '' });
        case 'PartiallyClaimed':
          return send({ tag: 'PartiallyClaimed', value: BigInt(payload.value ?? '0') });
        default:
          return send({ tag: 'Processing', value: undefined });
      }
    },
    (e) => {
      // Deferred for the same reason as the top up status subscription above.
      const failure =
        (e as { code?: string })?.code === 'NotFound'
          ? new PaymentStatusErr.PaymentNotFound()
          : new PaymentStatusErr.Unknown({ reason: String((e as Error)?.message ?? e) });

      queueMicrotask(() => interrupt(failure));
    },
  );
});

// --- Local Storage (native-bridged) ---

container.handleLocalStorageRead((key, { ok, err }) => {
  return callNative('localStorageRead', { key }).then(
    (result) => ok(result.value != null ? fromHex(result.value) : undefined),
    (e) => err(new StorageErr.Unknown({ reason: String(e) })),
  );
});

container.handleLocalStorageWrite((params, { ok, err }) => {
  const [key, value] = params;
  return callNative('localStorageWrite', { key, value: toHex(value) }).then(
    () => ok(undefined),
    (e) => err(new StorageErr.Unknown({ reason: String(e) })),
  );
});

container.handleLocalStorageClear((key, { ok, err }) => {
  return callNative('localStorageClear', { key }).then(
    () => ok(undefined),
    (e) => err(new StorageErr.Unknown({ reason: String(e) })),
  );
});

container.handleLocalStorageSubscribe((key, send, _interrupt) => {
  return subscribeNative(
    'localStorageSubscribe',
    { key },
    (payload: { value: string | null }) => send(payload.value != null ? fromHex(payload.value) : undefined),
  );
});

// --- Worker keep-alive operations (native-bridged) ---

container.handleWorkerBeginOperation((params, { ok, err }) => {
  return callNative('workerBeginOperation', { label: params.label }).then(
    (result) => ok({ id: result.operationId }),
    (e) => err(new WorkerErr.Unknown({ reason: String(e) })),
  );
});

container.handleWorkerEndOperation((params, { ok, err }) => {
  return callNative('workerEndOperation', { id: params.id }).then(
    () => ok(undefined),
    (e) => err(new WorkerErr.Unknown({ reason: String(e) })),
  );
});

// --- Locale (native-bridged) ---

container.handleLocaleSubscribe((_params, send, _interrupt) => {
  return subscribeNative(
    'localeSubscribe',
    {},
    (payload: { languageTag: string }) => send({ languageTag: payload.languageTag }),
  );
});

// --- Navigation (native-bridged) ---

container.handleNavigateTo((destination, { ok, err }) => {
    return callNative('navigateTo', { destination }).then(
      () => ok(undefined),
      (e) => err(new NavigateToErr.Unknown({ reason: String(e) })),
    );
});

console.log('Host container initialized');
