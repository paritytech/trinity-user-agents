import { createContainer } from '@novasamatech/host-container';
import type { CodecType, DerivationIndex, Provider, Subscription } from '@novasamatech/host-api';
import {
  ChatMessagePostingErr,
  CreateProofErr,
  CreateTransactionErr,
  CustomRendererNode,
  DeriveEntropyErr,
  GenericError,
  GetAliasErr,
  NavigateToErr,
  PaymentBalanceErr,
  PaymentRequestErr,
  PaymentStatusErr,
  PaymentTopUpErr,
  PaymentTopUpStatusErr,
  PreimageSubmitErr,
  GetUserIdErr,
  RequestCredentialsErr,
  ResourceAllocationErr,
  SignVrfErr,
  SigningErr,
  StatementProofErr,
  StorageErr,
  WorkerErr,
  fromHex,
  toHex,
} from '@novasamatech/host-api';
import { createNativeTransport } from './native-transport';
import { ConnectionManager } from './connection-manager';
import { WebRtcManager } from './webrtc-manager';
import { validateStatementTopics } from './statement-topics';

// =============================================================================
// Isolation: Capture private refs BEFORE locking down globals.
// The container IIFE closure keeps these inaccessible to product scripts.
// WebSocket is replaced with a blocking Proxy (see below). The container
// captures the native WebSocket and passes it to ConnectionManager directly.
// =============================================================================

// --- Network: capture real refs before lockdown ---
// The actual freezes are deferred until callNative is available (see below).
const _nativeFetch = window.fetch.bind(window);
const _NativeXMLHttpRequest = window.XMLHttpRequest;
const _NativeWebSocket = window.WebSocket;
const _NativeRTCPeerConnection = window.RTCPeerConnection;

const _BlockedWebSocket = new Proxy(window.WebSocket, {
  construct() {
    throw new TypeError('Network access is not allowed');
  },
});

// =============================================================================
// Isolation: Lock down globals so product scripts cannot access platform APIs.
// =============================================================================

function freezeAndDelete(obj: any, prop: string) {
  try {
    Object.defineProperty(obj, prop, {
      get: () => undefined,
      set() { /* silently ignore */ },
      configurable: false,
    });
  } catch {
    // Property may already be non-configurable; try delete as fallback
    try { delete obj[prop]; } catch { /* best effort */ }
  }
}

function freezeValue(obj: any, prop: string, value: any) {
  try {
    // Use a getter instead of a data property with writable:false.
    // A non-writable data property on the prototype chain prevents
    // descendant objects from shadowing it, which breaks polyfills
    // that create objects with window/self as prototype.
    Object.defineProperty(obj, prop, {
      get: () => value,
      set() { /* silently ignore */ },
      configurable: false,
    });
  } catch { /* best effort */ }
}

freezeValue(window, 'WebSocket', _BlockedWebSocket);

// --- Network: delete (no future permission path) ---
// RTCPeerConnection is permission-gated below, once callNative is available.
freezeAndDelete(window, 'EventSource');

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
      value: Object.freeze({
        register: () => { throw new Error('ServiceWorker is not available'); },
      }),
      writable: false,
      configurable: false,
    });
  } catch { /* best effort */ }
}

// --- DOM: block iframe creation ---
// TODO: temporary — allowing iframes while we explore a universal solution.
//       Re-enable/replace this block once that lands.
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
  window.webkit.messageHandlers.__container__.postMessage(json);
});

// --- Network: permission-gated fetch & XMLHttpRequest (deferred until callNative is available) ---
freezeValue(window, 'fetch', async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;

  const response = await callNative('allowNetworkAccess', { url });

  if (!response.allowed) {
    return Promise.reject(new TypeError('Network access is not allowed'));
  }

  return _nativeFetch(input, init);
});

freezeValue(window, 'XMLHttpRequest', function XMLHttpRequest(this: XMLHttpRequest) {
  const xhr = new _NativeXMLHttpRequest();
  const _open = xhr.open.bind(xhr);

  xhr.open = function (method: string, url: string | URL, ...rest: any[]) {
    const resolvedUrl = typeof url === 'string' ? url : url.href;

    callNative('allowNetworkAccess', { url: resolvedUrl }).then((response: any) => {
      if (!response.allowed) {
        xhr.dispatchEvent(new Event('error'));
        xhr.abort();
        return;
      }
      _open(method, url, ...rest);
    });
  } as typeof xhr.open;

  return xhr;
} as any);

// --- WebRTC: permission-gated RTCPeerConnection (deferred until callNative is available) ---
if (_NativeRTCPeerConnection) {
  console.log('[webRtc] RTCPeerConnection available, installing permission-gated proxy');
  const webRtcManager = new WebRtcManager(
    _NativeRTCPeerConnection,
    () => callNative('allowWebRtcAccess', {}).then((response: any) => response?.allowed === true),
  );
  freezeValue(window, 'RTCPeerConnection', webRtcManager.connectionClass);
} else {
  console.log('[webRtc] RTCPeerConnection unavailable, removing global');
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
  isCorrectEnvironment: () => true,
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

type WireDerivationIndex = CodecType<typeof DerivationIndex>;
type WireProductAccountId = [productId: string, derivationIndex: WireDerivationIndex];

/**
 * RFC-0022: native takes the account selector in its tolerant JSON form — a number for a plain
 * index, a 0x-hex string for a raw 32-byte one. Native re-expands `Index(n)` to `index_bytes(n)`.
 */
function toNativeDerivationIndex(index: WireDerivationIndex): number | string {
  return index.tag === 'Index' ? index.value : toHex(index.value);
}

function toNativeAccountId([productId, derivationIndex]: WireProductAccountId): [string, number | string] {
  return [productId, toNativeDerivationIndex(derivationIndex)];
}

// --- Account ---

container.handleAccountGet((account, { ok, err }) => {
  return callNative('accountGet', { account: toNativeAccountId(account) }).then(
    (result) => ok({
      publicKey: fromHex(result.publicKey),
    }),
    (e) => err(new RequestCredentialsErr.Unknown({ reason: String(e) })),
  );
});

container.handleGetUserId((_params, { ok, err }) => {
  return callNative('getUserId', {}).then(
    (result) => ok({ primaryUsername: result.primaryUsername }),
    (e) => {
      const msg = String(e);
      if (msg.includes('NotConnected')) return err(new GetUserIdErr.NotConnected());
      if (msg.includes('PermissionDenied')) return err(new GetUserIdErr.PermissionDenied());
      return err(new GetUserIdErr.Unknown({ reason: msg }));
    },
  );
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
  const [context, ring] = params;
  return callNative('accountGetAlias', {
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
        case 'Rejected':
          return err(new GetAliasErr.Rejected());
        default:
          return err(new GetAliasErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

container.handleAccountCreateProof((params, { ok, err }) => {
  const [context, ring, message] = params;
  return callNative('accountCreateProof', {
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
        case 'Rejected':
          return err(new CreateProofErr.Rejected());
        default:
          return err(new CreateProofErr.Unknown({ reason: String(e?.message ?? e) }));
      }
    },
  );
});

// --- Sign VRF (RFC-0023) ---

container.handleAccountSignVrf((params, { ok, err }) => {
  return callNative('accountSignVrf', {
    account: toNativeAccountId(params.account),
    transcriptLabel: toHex(params.transcriptLabel),
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

container.handleGetLegacyAccounts((_params, { ok, err }) => {
  return callNative('getNonProductAccounts', {}).then(
    (result: { publicKey: string; name?: string }[]) => ok(result.map((acc) => ({
      publicKey: fromHex(acc.publicKey),
      name: acc.name ?? undefined,
    }))),
    (e) => err(new RequestCredentialsErr.Unknown({ reason: String(e) })),
  );
});

// --- Connection ---

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

const connectionManager = new ConnectionManager(_NativeWebSocket);

container.handleChainConnection((genesisHash) => {
  return (onMessage) => {
    const entry = connectionManager.add(genesisHash, onMessage);

    callNative('chainNodes', { genesisHash }).then((urls: string[]) => {
      console.log('[chainConnection] genesisHash:', genesisHash, 'urls:', urls);
      entry.urls = urls;
      connectionManager.connect(entry, genesisHash);
    });

    return {
      send(message: string) {
        if (entry.inner) entry.inner.send(message);
        else entry.buffer.push(message);
      },
      disconnect() {
        connectionManager.disconnect(genesisHash);
      },
    };
  };
});

(window as any).__pauseConnections__ = () => connectionManager.pauseAll();
(window as any).__resumeConnections__ = () => connectionManager.resumeAll();

// --- Signing ---

container.handleSignPayload(async ({ account, payload }, { ok, err }) => {
  try {
    const result = await callNative('signPayload', { account: toNativeAccountId(account), ...payload });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch {
    return err(new SigningErr.Rejected());
  }
});

container.handleSignRaw(async ({ account, payload }, { ok, err }) => {
  try {
    const nativeData = payload.tag === 'Bytes'
      ? { data: toHex(payload.value) }
      : { payload: payload.value };
    const result = await callNative('signRaw', { account: toNativeAccountId(account), ...nativeData });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch {
    return err(new SigningErr.Rejected());
  }
});

container.handleCreateTransaction(async (payload, { ok, err }) => {
  try {
    const result = await callNative('createTransaction', {
      signer: toNativeAccountId(payload.signer),
      genesisHash: payload.genesisHash,
      callData: toHex(payload.callData),
      extensions: payload.extensions.map((e) => ({
        id: e.id,
        explicit: toHex(e.extra),
        implicit: toHex(e.additionalSigned),
      })),
      txExtVersion: payload.txExtVersion,
    });
    return ok(fromHex(result.signedTx));
  } catch (e) {
    return err(new CreateTransactionErr.Unknown({ reason: String(e) }));
  }
});

// --- Legacy Signing (identity account) ---

container.handleSignPayloadWithLegacyAccount(async ({ signer, payload }, { ok, err }) => {
  try {
    const result = await callNative('signPayloadLegacy', { account: signer, ...payload });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch {
    return err(new SigningErr.Rejected());
  }
});

container.handleSignRawWithLegacyAccount(async ({ signer, payload }, { ok, err }) => {
  try {
    const nativeData = payload.tag === 'Bytes'
      ? { data: toHex(payload.value) }
      : { payload: payload.value };
    const result = await callNative('signRawLegacy', { account: signer, ...nativeData });
    return ok({ signature: result.signature, signedTransaction: result.signedTx ?? undefined });
  } catch {
    return err(new SigningErr.Rejected());
  }
});

container.handleCreateTransactionWithLegacyAccount(async (payload, { ok, err }) => {
  try {
    const result = await callNative('createTransactionLegacy', {
      signer: toHex(payload.signer),
      genesisHash: payload.genesisHash,
      callData: toHex(payload.callData),
      extensions: payload.extensions.map((e) => ({
        id: e.id,
        explicit: toHex(e.extra),
        implicit: toHex(e.additionalSigned),
      })),
      txExtVersion: payload.txExtVersion,
    });
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

container.handleThemeSubscribe((_params, send, _interrupt) => {
  return subscribeNative(
    'themeSubscribe',
    {},
    (result: any) => send({ name: { tag: 'Custom', value: result.name }, variant: result.variant }),
    () => send({ name: { tag: 'Default', value: undefined }, variant: 'Dark' }),
  );
});

// --- Locale ---

// Native emits the host UI language as a BCP 47 tag, once on subscribe and on change.
container.handleLocaleSubscribe((_params, send, _interrupt) => {
  return subscribeNative('localeSubscribe', {}, (result: { languageTag: string }) => {
    send({ languageTag: result.languageTag });
  });
});

// --- Login ---

container.handleRequestLogin((_params, { ok }) => {
  return ok('alreadyConnected');
});

// --- Chat: Post Message ---

container.handleChatPostMessage(async (params, { ok, err }) => {
  const { payload } = params;
  const chatId = (params as any).roomId;
  try {
    switch (payload.tag) {
      case 'Text': {
        const result = await callNative('chatSendTextMessage', { text: payload.value, chatId });
        return ok({ messageId: result.messageId });
      }
      case 'Custom': {
        const result = await callNative('chatSendCustomMessage', {
          messageType: payload.value.messageType,
          payloadHex: toHex(payload.value.payload),
          chatId,
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

// --- Chat: Room Management ---

container.handleChatCreateRoom(async (params, { ok, err }) => {
  try {
    const result = await callNative('chatCreateRoom', params);
    return ok({ status: result.status });
  } catch (e) {
    return err({ reason: String(e) } as any);
  }
});

container.handleChatListSubscribe((_params, send, _interrupt) => {
  const unsub = subscribeNative('chatSubscribeRooms', {}, (rooms: any) => {
    send(rooms);
  });
  return unsub;
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
  chatActionSend = send;
  return () => { chatActionSend = null; };
});

(window as any).dispatchChatAction = (
  roomId: string,
  messageId: string,
  actionId: string,
  payloadHex?: string,
) => {
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
  chatActionSend?.({
    roomId,
    peer: 'native',
    payload: {
      tag: 'MessagePosted',
      value: { tag: 'Text', value: text },
    },
  });
};

// --- Statement Store ---

container.handleStatementStoreSubscribe((filter, send, interrupt) => {
  const topicError = validateStatementTopics(filter.value);
  if (topicError) {
    console.error(topicError);
    interrupt(undefined);
    return () => {};
  }

  const topicsHex = filter.value.map((t) => toHex(t));
  const wireFilter = filter.tag === 'MatchAll' ? { matchAll: topicsHex } : { matchAny: topicsHex };
  const unsub = subscribeNative(
    'statementStoreSubscribe',
    { filter: wireFilter },
    (page: { statements: any[]; isComplete: boolean }) => {
      send({
        statements: page.statements.map((s) => ({
          proof: { tag: s.proof.tag as 'Sr25519', value: { signature: fromHex(s.proof.signature), signer: fromHex(s.proof.signer) } },
          decryptionKey: undefined,
          expiry: s.expiry != null ? BigInt(s.expiry) : undefined,
          channel: s.channel != null ? fromHex(s.channel) : undefined,
          topics: (s.topics as string[]).map((t: string) => fromHex(t)),
          data: s.data != null ? fromHex(s.data) : undefined,
        })),
        isComplete: page.isComplete,
      });
    },
  );
  return unsub;
});

// TODO: Remove when all migrate to authorized version
//  Switch to product-derived account once the chain supports granting allowances (e.g. zk vouchers).
container.handleStatementStoreCreateProof(async ([account, statement], { ok, err }) => {
  const topicError = validateStatementTopics(statement.topics);
  if (topicError) return err(new StatementProofErr.Unknown({ reason: topicError }));

  try {
    const result = await callNative('createStatementProof', {
      account: toNativeAccountId(account),
      channel: statement.channel ? toHex(statement.channel) : undefined,
      expiry: statement.expiry?.toString() ?? undefined,
      topics: statement.topics.map((t) => toHex(t)),
      data: statement.data ? toHex(statement.data) : undefined,
    });
    return ok({
      tag: result.tag as 'Sr25519',
      value: { signature: fromHex(result.signature), signer: fromHex(result.signer) },
    });
  } catch {
    return err(new StatementProofErr.UnableToSign());
  }
});

container.handleStatementStoreSubmit(async (statement, { ok, err }) => {
  const topicError = validateStatementTopics(statement.topics);
  if (topicError) return err(new GenericError({ reason: topicError }));

  try {
    const proofValue = (statement.proof as any).value;
    await callNative('statementStoreSubmit', {
      proof: {
        tag: statement.proof.tag,
        signature: toHex(proofValue.signature),
        signer: toHex(proofValue.signer),
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

// --- Preimage Lookup ---

container.handlePreimageLookupSubscribe((hashHex, send, _interrupt) => {
  callNative('preimageLookup', { hash: hashHex }).then(
    (result: { data?: string }) => send(result.data ? fromHex(result.data) : null),
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

// --- Permissions ---

container.handleDevicePermission(async (capability, { ok, err }) => {
  try {
    const result = await callNative('devicePermission', { capability });
    return ok(result);
  } catch (e) {
    return err(new GenericError({ reason: String(e) }));
  }
});

container.handlePermission(async (requests, { ok, err }) => {
  console.log('[handlePermission] raw requests:', JSON.stringify(requests));
  console.log('[handlePermission] isArray:', Array.isArray(requests));
  console.log('[handlePermission] typeof:', typeof requests);
  try {
    const payload = (Array.isArray(requests) ? requests : [requests]).map((r: any) => ({
      tag: r.tag,
      value: r.tag === 'Remote' ? r.value : undefined,
    }));
    console.log('[handlePermission] payload to native:', JSON.stringify(payload));
    const result = await callNative('remotePermission', payload);
    console.log('[handlePermission] native result:', JSON.stringify(result));
    return ok(result);
  } catch (e) {
    console.log('[handlePermission] error:', String(e));
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Local Storage (native-bridged) ---

container.handleLocalStorageRead((key, { ok, err }) => {
  return callNative('localStorageRead', { key }).then(
    (result) => ok(result.value != null ? fromHex(result.value) : undefined),
    (e) => err(new StorageErr.Unknown({ reason: String(e) })),
  );
});

container.handleLocalStorageWrite(([key, value], { ok, err }) => {
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

// Native emits the current value first, then one item per later write/clear.
// `undefined` (Option None) means the key is cleared or absent.
container.handleLocalStorageSubscribe(({ key }, send, _interrupt) => {
  return subscribeNative('localStorageSubscribe', { key }, (result: { value?: string | null }) => {
    send({ value: result.value != null ? fromHex(result.value) : undefined });
  });
});

// --- Worker operations (keep-alive; native ref-counts and persists) ---

container.handleWorkerBeginOperation(async ({ label }, { ok, err }) => {
  try {
    const result = await callNative('workerBeginOperation', { label: label ?? undefined });
    return ok({ id: result.id });
  } catch (e) {
    return err(new WorkerErr.Unknown({ reason: String(e) }));
  }
});

// Idempotent: ending an unknown or already-ended id still succeeds.
container.handleWorkerEndOperation(async ({ id }, { ok, err }) => {
  try {
    await callNative('workerEndOperation', { id });
    return ok(undefined);
  } catch (e) {
    return err(new WorkerErr.Unknown({ reason: String(e) }));
  }
});

// --- Navigation (native-bridged) ---

container.handleNavigateTo((destination, { ok, err }) => {
  return callNative('navigateTo', { destination }).then(
    () => ok(undefined),
    (e) => err(new NavigateToErr.Unknown({ reason: String(e) })),
  );
});

// --- Push Notification ---

container.handlePushNotification(async (params, { ok, err }) => {
  try {
    const scheduledAt = params.scheduledAt !== undefined ? Number(params.scheduledAt) : undefined;
    const result = await callNative('pushNotification', {
      text: params.text,
      deeplink: params.deeplink,
      scheduledAtMs: scheduledAt,
    });
    return ok(result.notificationId);
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
    await callNative('cancelPushNotification', { identifier });
    return ok(undefined);
  } catch (e) {
    return err(new GenericError({ reason: String(e) }));
  }
});

// --- Entropy Derivation ---

container.handleDeriveEntropy(async (key, { ok, err }) => {
  try {
    const result = await callNative('deriveEntropy', { key: toHex(key) });
    return ok(fromHex(result.entropy));
  } catch (e) {
    return err(new DeriveEntropyErr.Unknown({ reason: String(e) }));
  }
});

// --- Payments ---

container.handlePaymentBalanceSubscribe((_params, send, interrupt) => {
  return subscribeNative(
    'paymentBalanceSubscribe',
    {},
    (payload: { available: string }) => {
      send({ available: BigInt(payload.available) });
    },
    () => interrupt(new PaymentBalanceErr.Unknown({ reason: 'subscription interrupted' })),
  );
});

container.handlePaymentRequest(async (params, { ok, err }) => {
  try {
    await callNative('paymentRequest', {
      idHex: toHex(params.id),
      amount: params.amount.toString(),
      destinationHex: toHex(params.destination),
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
      nativeParams.sourceKeyListHex = params.source.value.map((key: Uint8Array) => toHex(key));
    } else {
      nativeParams.sourceKeyHex = toHex(params.source.value);
    }
    await callNative('paymentTopUp', nativeParams);
    return ok(undefined);
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
      // Deferred for the same reason as the top-up status subscription above.
      const failure =
        (e as { code?: string })?.code === 'NotFound'
          ? new PaymentStatusErr.PaymentNotFound()
          : new PaymentStatusErr.Unknown({ reason: String((e as Error)?.message ?? e) });

      queueMicrotask(() => interrupt(failure));
    },
  );
});

console.log('Host container initialized');
