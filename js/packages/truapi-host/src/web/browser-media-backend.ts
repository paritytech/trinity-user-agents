import { err, ok } from "neverthrow";
import type {
  GenericError,
  HostMediaError,
  MediaAudioRoute,
  MediaLocalState,
  MediaLocalTracks,
  MediaOperationFailure,
  MediaRect,
  MediaRemoteState,
  MediaSurface,
  MediaTrackState,
  MediaViewport,
  Result,
} from "@parity/truapi";
import type {
  MediaBackendCapabilities,
  MediaBackendCommand,
  MediaBackendEvent,
  MediaBackendResponse,
  MediaBackendPeerState,
  MediaConsentRequest,
  MediaPlatform,
  MediaRevokedPermission,
  MediaRevocationSource,
  ProductContext,
} from "../generated/host-callbacks.js";

type SessionId = Extract<
  MediaBackendCommand,
  { tag: "OpenSession" }
>["value"]["sessionId"];
type ParticipantId = Extract<
  MediaBackendCommand,
  { tag: "CreatePeer" }
>["value"]["participantId"];
type OperationId = Extract<
  MediaBackendCommand,
  { tag: "CommitOperation" }
>["value"]["operationId"];
type EventResult = Result<MediaBackendEvent, GenericError>;
type TrackKind = "microphone" | "camera" | "screen";
type Tracks = Partial<Record<TrackKind, MediaStreamTrack>>;
const TRACK_KINDS: readonly TrackKind[] = ["microphone", "camera", "screen"];
const HTML_NS = "http://www.w3.org/1999/xhtml";
const MAX_OPERATIONS = 8192;
const MAX_ISSUED_SESSIONS = 256;
const MAX_ISSUED_PEERS = 2048;
const EVENT_CAPACITY = 128;
const MAX_SUBSCRIPTIONS = 8;
const MAX_RUNTIMES = 64;
const MAX_ICE_CANDIDATES = 256;
const CONNECTION_TIMEOUT = 30_000;

/** Coordinates are in the trusted compositor mount's CSS coordinate system. */
export interface BrowserMediaGeometry {
  left: number;
  top: number;
  /** Logical product viewport size, before the uniform mount-space scale. */
  width: number;
  height: number;
  scale: number;
  /** Physical pixels per product logical pixel, including visual viewport zoom. */
  deviceScale: number;
  /** Host-approved visible rectangle in product logical coordinates. */
  clip: MediaRect;
  visible: boolean;
}

/**
 * Trusted host integration only. Never construct these options from a product
 * request or expose this object, backend, media elements, or callbacks to a product.
 *
 * SECURITY REQUIREMENTS (discovery cannot establish these):
 * - window/document must be the privileged host realm, not the product realm.
 * - The product must be in a separately isolated iframe/Gecko browser. Same-origin
 *   unsandboxed iframes, a product in the host document, and a closed shadow root
 *   alone are NOT isolation. Prevent product access to host DOM and JS, host
 *   capture APIs, and product-facing screenshot/renderer APIs that include these
 *   sibling planes. Independent product RTCPeerConnection use may remain
 *   available; it must never receive host Media tracks, SDP, ICE, keys, or frames.
 *   HTML iframes must explicitly deny microphone, camera, display-capture and
 *   fullscreen in their allow attribute. Keep that isolation across every
 *   navigation; a host origin must never serve product-authored executable code.
 * - getCompositorMount returns the product element's immediate, trusted parent.
 *   It is a positioned isolated stacking context. The complete product element
 *   is a positioned stacking context at z-index 1, planes occupy 0 and 2. Trusted
 *   chrome and indicatorMount must be ABOVE that entire context. Do not permit
 *   product fullscreen/top-layer content to cover or intercept trusted controls.
 * - All mounts remain host-owned. The indicator remains visible and interactive
 *   even for hidden/detached/background products; it is not inside a product view
 *   or a compositor mount that can be hidden with the product.
 * - Supply measureViewport for transformed/clipped native embeddings. Its clip
 *   must include every host occlusion and approved visible region, never enlarge
 *   product visibility. Only axis-aligned uniform transforms are representable.
 *
 * isProductIsolated is a host policy assertion, not a heuristic origin check.
 * It must account for the current navigation/sandbox/process and screenshot
 * policy protecting host Media, not require a blanket product RTC prohibition.
 * A host must detach before replacing/navigation-changing an attachment.
 */
export interface BrowserMediaBackendOptions {
  window: Window;
  document: Document;
  /**
   * Trusted conversion of plain WebIDL dictionaries into window's realm.
   * Required for Gecko system-module/Xray callers: (value) => Cu.cloneInto(value,
   * window). Same-realm browser hosts omit it. Values include private ICE/SDP;
   * this callback and its results must never be exposed to product code.
   */
  toWebIdlValue?<T>(value: T): T;
  productId: string;
  getProductElement(runtimeId: bigint): Element | null;
  getCompositorMount(runtimeId: bigint): Element | null;
  isProductIsolated(product: Element, runtimeId: bigint): boolean;
  indicatorMount: Element;
  /**
   * Mandatory trusted consent UI, not a product permission prompt. Return a
   * boolean only for an actual user answer. Dismissal must reject AbortError,
   * never return false. AbortSignal must synchronously dismiss pending UI,
   * including UI scheduled to mount later. Settle only after that UI is removed.
   * The core alone caches/persists decisions; this hook must not persist grants,
   * grant raw iframe capture permissions, change independent RTC policy,
   * navigate, or reload the product.
   */
  requestConsent(
    request: MediaConsentRequest,
    context: BrowserMediaConsentContext,
  ): Promise<boolean>;
  /**
   * Host-owned TURN configuration; credentials never enter product values.
   * Peers are always relay-only, so this must name a reachable TURN relay.
   */
  iceServers:
    | readonly RTCIceServer[]
    | ((runtimeId: bigint) => Promise<readonly RTCIceServer[]>);
  measureViewport?(
    product: Element,
    mount: Element,
    runtimeId: bigint,
  ): BrowserMediaGeometry | undefined;
  /** Advisory preference, resolved only against trusted host device knowledge. */
  resolveAudioOutput?(preference: MediaAudioRoute | undefined): Promise<
    | {
        sinkId: string;
        route: MediaAudioRoute;
      }
    | undefined
  >;
}

/** Operation identity and cancellation owned by the trusted host runtime. */
export interface BrowserMediaConsentContext {
  readonly productId: string;
  readonly runtimeId: bigint;
  readonly operationId: OperationId;
  readonly signal: AbortSignal;
}

export interface BrowserMediaBackend extends MediaPlatform {
  /** Authorize the current host-selected product attachment for this runtime. */
  attach(runtimeId: bigint): void;
  /** Clear every layout and advance the viewport epoch; audio continues. */
  detach(runtimeId: bigint): void;
  /** Call synchronously when native embedding geometry/occlusion changes. */
  refreshViewport(runtimeId: bigint): void;
  /**
   * Trusted permission integration. Settings revocation defaults to Product;
   * native OS observers must pass OperatingSystem to preserve the core grant.
   */
  revokePermission(
    runtimeId: bigint,
    permission: MediaRevokedPermission,
    source?: MediaRevocationSource,
  ): void;
  dispose(): void;
}

interface EventQueue {
  items: EventResult[];
  waiter?: (result: IteratorResult<EventResult>) => void;
  closed: boolean;
}
/** A composited picture: one <video> kept alive across layouts of its track. */
interface Plane {
  wrapper: HTMLDivElement;
  rounded: HTMLDivElement;
  video: HTMLVideoElement;
  track: MediaStreamTrack;
}
interface Attachment {
  product: Element;
  mount: Element;
  below: HTMLDivElement;
  above: HTMLDivElement;
  /** Rendered pictures by placement, session key and surface id. */
  planes: Map<string, Plane>;
  geometry?: BrowserMediaGeometry;
  signature: string;
}
interface Runtime {
  id: bigint;
  closed: boolean;
  sessions: Map<string, Session>;
  operations: Map<string, Operation>;
  queues: Set<EventQueue>;
  pendingEvents: MediaBackendEvent[];
  viewportRevision: bigint;
  viewport?: MediaViewport;
  attachment?: Attachment;
  dirty: boolean;
}
interface Consent {
  fingerprint: string;
  controller?: AbortController;
  promise: Promise<MediaBackendResponse>;
}
interface Operation {
  key: string;
  session?: Session;
  revision: bigint;
  requested?: MediaLocalTracks;
  tracks: Tracks;
  opening: boolean;
  status:
    | "admitted"
    | "consenting"
    | "preparing"
    | "prepared"
    | "committing"
    | "committed"
    | "cancelled"
    | "failed";
  promise?: Promise<MediaBackendResponse>;
  result?: MediaBackendResponse;
  cancel: () => void;
  cancelled: Promise<never>;
  pickerButton?: HTMLButtonElement;
  consents: Partial<Record<MediaConsentRequest["tag"], Consent>>;
  activeConsent?: Consent;
}
interface Session {
  runtime: Runtime;
  id: SessionId;
  key: string;
  closed: boolean;
  committed: boolean;
  requested: MediaLocalTracks;
  tracks: Tracks;
  revision: bigint;
  admittedRevision: bigint;
  pending?: Operation;
  peers: Map<string, Peer>;
  serial: Promise<unknown>;
  layoutRevision?: bigint;
  layoutFingerprint?: string;
  layoutViewportRevision?: bigint;
  surfaces: MediaSurface[];
  queued?: MediaSurface[];
  indicator: HTMLDivElement;
  label: HTMLSpanElement;
  stopScreen: HTMLButtonElement;
  resumeAudio: HTMLButtonElement;
  route?: MediaAudioRoute;
  routeGeneration: number;
  routeSerial: Promise<unknown>;
  listeners: (() => void)[];
}
interface Peer {
  id: ParticipantId;
  key: string;
  session: Session;
  closed: boolean;
  pc?: RTCPeerConnection;
  ready?: Promise<void>;
  serial: Promise<unknown>;
  pendingCommands: number;
  slots: RTCRtpTransceiver[];
  remote: Tracks;
  seen: Set<TrackKind>;
  candidates: RTCIceCandidateInit[];
  offerer: boolean;
  makingOffer: boolean;
  ignoreOffer: boolean;
  remoteOfferSeen: boolean;
  state: MediaBackendPeerState;
  audio: HTMLAudioElement;
  playbackBlocked: boolean;
  timer?: number;
}

class CommandRejected extends Error {
  constructor(readonly failure: MediaOperationFailure) {
    super("Media command rejected");
  }
}
function domainFailure(error: HostMediaError): CommandRejected {
  return new CommandRejected({ tag: "Domain", value: { error } });
}
function failure(reason = "HostFailure"): Error {
  // Expected failures carry typed data, never a diagnostic for core to parse.
  switch (reason) {
    case "Denied":
      return new CommandRejected({ tag: "Denied" });
    case "ProductMismatch":
    case "InvalidHandle":
    case "SessionEnded":
    case "InvalidState":
    case "OperationConflict":
    case "OperationCancelled":
    case "InvalidOperation":
    case "DeviceUnavailable":
    case "CaptureCancelled":
    case "EventOverflow":
    case "SurfaceUnavailable":
    case "InvalidSurface":
      return domainFailure({ tag: reason });
    default:
      return new Error("Media backend failed");
  }
}
function stopTracks(tracks: Tracks): void {
  for (const track of Object.values(tracks)) track?.stop();
}
function trackState(track: MediaStreamTrack | undefined): MediaTrackState {
  if (!track) return "Off";
  return track.readyState === "ended" || track.muted ? "Interrupted" : "Live";
}
function emptyIntent(): MediaLocalTracks {
  return { microphone: false, camera: false, screen: false };
}
function rectValid(rect: MediaRect): boolean {
  return (
    Number.isInteger(rect.x) &&
    rect.x >= -2147483648 &&
    rect.x <= 2147483647 &&
    Number.isInteger(rect.y) &&
    rect.y >= -2147483648 &&
    rect.y <= 2147483647 &&
    Number.isInteger(rect.width) &&
    rect.width >= 0 &&
    rect.width <= 4294967295 &&
    Number.isInteger(rect.height) &&
    rect.height >= 0 &&
    rect.height <= 4294967295 &&
    Number.isSafeInteger(rect.x + rect.width) &&
    Number.isSafeInteger(rect.y + rect.height)
  );
}
/** Only TURN relay candidates may leave the host; anything else names an address. */
const RELAY_CANDIDATE = /(?:^|\s)typ\s+relay(?:\s|$)/;
function intersect(...rects: MediaRect[]): MediaRect {
  const x = Math.max(...rects.map((rect) => rect.x));
  const y = Math.max(...rects.map((rect) => rect.y));
  return {
    x,
    y,
    width: Math.max(
      0,
      Math.min(...rects.map((rect) => rect.x + rect.width)) - x,
    ),
    height: Math.max(
      0,
      Math.min(...rects.map((rect) => rect.y + rect.height)) - y,
    ),
  };
}

/** A host-only DOM/Gecko implementation. No raw media leaves this closure. */
export function createBrowserMediaBackend(
  options: BrowserMediaBackendOptions,
): BrowserMediaBackend {
  const win = options.window as Window & typeof globalThis;
  const doc = options.document;
  const productId = options.productId;
  const permissionCleanups: (() => void)[] = [];
  const runtimes = new Map<bigint, Runtime>();
  let disposed = false;
  let animationFrame: number | undefined;
  const capabilities: MediaBackendCapabilities = {
    supported: false,
    maxSessions: 1,
    maxRemoteParticipants: 5,
    maxSurfacesPerSession: 12,
  };

  function html<K extends keyof HTMLElementTagNameMap>(
    tag: K,
  ): HTMLElementTagNameMap[K] {
    // createElement in a chrome/XUL document would create inert XUL elements.
    return doc.createElementNS(HTML_NS, tag) as HTMLElementTagNameMap[K];
  }
  function webIdl<T>(value: T): T {
    return options.toWebIdlValue ? options.toWebIdlValue(value) : value;
  }
  function trackStream(track: MediaStreamTrack): MediaStream {
    // A module-realm JS array can appear empty through a Gecko Window Xray.
    // Keep native track objects native instead of cloning them as dictionary data.
    const stream = new win.MediaStream();
    stream.addTrack(track);
    return stream;
  }
  const videoProbe = html("video");
  function supported(): boolean {
    const devices = win.navigator.mediaDevices;
    const video = videoProbe;
    const rtc = win.RTCPeerConnection?.prototype;
    const track = win.MediaStreamTrack?.prototype;
    return (
      !disposed &&
      doc.defaultView === win &&
      win.isSecureContext &&
      productId.length > 0 &&
      options.indicatorMount.ownerDocument === doc &&
      options.indicatorMount.isConnected &&
      // Screen capture is optional. Mobile Safari can call and render
      // host-composited video without exposing getDisplayMedia.
      typeof devices?.getUserMedia === "function" &&
      typeof devices.getSupportedConstraints === "function" &&
      devices.getSupportedConstraints().echoCancellation === true &&
      typeof win.MediaStream === "function" &&
      typeof win.MediaStream.prototype.addTrack === "function" &&
      typeof track?.clone === "function" &&
      typeof track.getSettings === "function" &&
      typeof track.stop === "function" &&
      typeof win.RTCPeerConnection === "function" &&
      typeof rtc?.addTransceiver === "function" &&
      typeof rtc.setLocalDescription === "function" &&
      typeof rtc.setRemoteDescription === "function" &&
      typeof rtc.addIceCandidate === "function" &&
      typeof rtc.createOffer === "function" &&
      typeof rtc.createAnswer === "function" &&
      typeof win.RTCRtpSender?.prototype.replaceTrack === "function" &&
      typeof win.requestAnimationFrame === "function" &&
      typeof win.cancelAnimationFrame === "function" &&
      typeof video.play === "function" &&
      typeof video.pause === "function" &&
      "srcObject" in video &&
      typeof win.DOMMatrixReadOnly === "function" &&
      win.CSS?.supports("object-fit", "cover") &&
      win.CSS.supports("isolation", "isolate") &&
      win.CSS.supports("border-radius", "1px") &&
      typeof options.getProductElement === "function" &&
      typeof options.getCompositorMount === "function" &&
      typeof options.isProductIsolated === "function" &&
      typeof options.requestConsent === "function" &&
      typeof win.AbortController === "function"
    );
  }
  function authorize(product: ProductContext): void {
    if (disposed || product.productId !== productId)
      throw failure("ProductMismatch");
  }
  function runtime(id: bigint): Runtime {
    const existing = runtimes.get(id);
    if (existing) {
      if (existing.closed) throw failure("RuntimeClosed");
      return existing;
    }
    if (runtimes.size >= MAX_RUNTIMES)
      throw domainFailure({
        tag: "ResourceExhausted",
        value: { resource: "Sessions" },
      });
    const result: Runtime = {
      id,
      closed: false,
      sessions: new Map(),
      operations: new Map(),
      queues: new Set(),
      viewportRevision: 0n,
      dirty: false,
      pendingEvents: [],
    };
    runtimes.set(id, result);
    return result;
  }
  function requireSession(rt: Runtime, id: SessionId): Session {
    const session = rt.sessions.get(id);
    if (!session || session.closed) throw failure("InvalidHandle");
    return session;
  }
  function alive(session: Session): boolean {
    return !disposed && !session.runtime.closed && !session.closed;
  }
  function requireAlive(session: Session): void {
    if (!alive(session)) throw failure("SessionEnded");
  }
  function peerAlive(peer: Peer): boolean {
    return alive(peer.session) && !peer.closed;
  }
  function push(queue: EventQueue, item: EventResult): boolean {
    if (queue.closed) return true;
    if (queue.waiter) {
      const waiter = queue.waiter;
      queue.waiter = undefined;
      waiter({ done: false, value: item });
      return true;
    }
    if (queue.items.length >= EVENT_CAPACITY) return false;
    queue.items.push(item);
    return true;
  }
  function finishQueue(queue: EventQueue, reason?: string): void {
    queue.items.length = 0;
    if (reason) push(queue, err({ reason: `Media:${reason}` }));
    queue.closed = true;
    if (queue.waiter) {
      queue.waiter({ done: true, value: undefined });
      queue.waiter = undefined;
    }
  }
  function emit(rt: Runtime, event: MediaBackendEvent): void {
    if (rt.closed) return;
    if (!rt.queues.size) {
      // A queue opens with the then-current viewport; replaying buffered
      // viewport events after it would deliver superseded attachments (a
      // revisionless detach after the current viewport leaves the core with
      // none). Viewport state needs no buffering, only the other observations.
      if (event.tag === "ViewportChanged") return;
      if (rt.pendingEvents.length >= EVENT_CAPACITY - 1)
        closeRuntime(rt, "EventOverflow");
      else rt.pendingEvents.push(event);
      return;
    }
    for (const queue of rt.queues) {
      if (!push(queue, ok(event))) {
        // Losing signaling or an authoritative state observation is unsafe.
        // Fail the runtime and every subscriber, never silently drop an event.
        closeRuntime(rt, "EventOverflow");
        return;
      }
    }
  }
  function eventStream(rt: Runtime): AsyncIterable<EventResult> {
    return {
      [Symbol.asyncIterator]() {
        if (rt.closed || rt.queues.size >= MAX_SUBSCRIPTIONS) {
          let sent = false;
          return {
            async next(): Promise<IteratorResult<EventResult>> {
              if (sent) return { done: true, value: undefined };
              sent = true;
              return {
                done: false,
                value: err({ reason: "Media:ResourceExhausted" }),
              };
            },
          };
        }
        const queue: EventQueue = { items: [], closed: false };
        rt.queues.add(queue);
        push(
          queue,
          ok({ tag: "ViewportChanged", value: { viewport: rt.viewport } }),
        );
        for (const event of rt.pendingEvents.splice(0)) {
          if (!push(queue, ok(event))) {
            closeRuntime(rt, "EventOverflow");
            break;
          }
        }
        return {
          next(): Promise<IteratorResult<EventResult>> {
            const item = queue.items.shift();
            if (item) return Promise.resolve({ done: false, value: item });
            if (queue.closed)
              return Promise.resolve({ done: true, value: undefined });
            if (queue.waiter) return Promise.reject(failure("InvalidState"));
            return new Promise((resolve) => {
              queue.waiter = resolve;
            });
          },
          async return(): Promise<IteratorResult<EventResult>> {
            finishQueue(queue);
            rt.queues.delete(queue);
            return { done: true, value: undefined };
          },
        };
      },
    };
  }

  function localState(session: Session): MediaLocalState {
    const facing = session.tracks.camera?.getSettings().facingMode;
    return {
      microphone: trackState(session.tracks.microphone),
      camera: trackState(session.tracks.camera),
      screen: trackState(session.tracks.screen),
      cameraKind: session.tracks.camera
        ? facing === "user"
          ? "Front"
          : facing === "environment"
            ? "Rear"
            : "Other"
        : undefined,
      audioRoute: session.route,
    };
  }
  function updateIndicator(session: Session): void {
    if (!alive(session)) return;
    const state = localState(session);
    session.label.textContent = `${productId} — ${session.committed ? "Call active" : "Preparing call"}; microphone ${state.microphone.toLowerCase()}, camera ${state.camera.toLowerCase()}, screen ${state.screen.toLowerCase()}${session.pending ? "; capture change pending" : ""}`;
    session.stopScreen.hidden = !session.tracks.screen;
    session.resumeAudio.hidden = ![...session.peers.values()].some(
      (peer) => peer.playbackBlocked && !peer.closed,
    );
  }
  function publishLocal(session: Session): void {
    if (!alive(session) || !session.committed) return;
    updateIndicator(session);
    session.runtime.dirty = true;
    scheduleFrame();
    emit(session.runtime, {
      tag: "LocalStateChanged",
      value: {
        sessionId: session.id,
        intentRevision: session.revision,
        state: localState(session),
      },
    });
  }
  function serializeSession<T>(
    session: Session,
    action: () => Promise<T>,
  ): Promise<T> {
    const next = session.serial.then(() => {
      requireAlive(session);
      return action();
    });
    session.serial = next.catch(() => undefined);
    return next;
  }
  function cancelOperation(op: Operation): void {
    if (
      op.status !== "admitted" &&
      op.status !== "consenting" &&
      op.status !== "preparing" &&
      op.status !== "prepared"
    )
      return;
    op.status = "cancelled";
    op.activeConsent?.controller?.abort();
    op.cancel();
    stopTracks(op.tracks);
    op.tracks = {};
    op.pickerButton?.remove();
    if (op.session?.pending === op) op.session.pending = undefined;
    if (op.opening && op.session && !op.session.committed)
      closeSession(op.session);
    else if (op.session) updateIndicator(op.session);
  }
  function operation(rt: Runtime, id: OperationId): Operation {
    const opKey = id;
    const existing = rt.operations.get(opKey);
    if (existing) {
      if (existing.status === "cancelled") throw failure("OperationCancelled");
      if (existing.status !== "admitted") throw failure("OperationConflict");
      return existing;
    }
    if (rt.operations.size >= MAX_OPERATIONS)
      throw domainFailure({
        tag: "ResourceExhausted",
        value: { resource: "Operations" },
      });
    let cancel!: () => void;
    const cancelled = new Promise<never>((_, reject) => {
      cancel = () => reject(failure("OperationCancelled"));
    });
    // A cancellation can precede the first capture or the commit callback.
    void cancelled.catch(() => undefined);
    const op: Operation = {
      key: opKey,
      revision: 0n,
      tracks: {},
      opening: false,
      status: "admitted",
      cancel,
      cancelled,
      consents: {},
    };
    rt.operations.set(opKey, op);
    return op;
  }
  async function requestConsent(
    rt: Runtime,
    id: OperationId,
    request: MediaConsentRequest,
  ): Promise<MediaBackendResponse> {
    const existing = rt.operations.get(id);
    if (existing?.status === "cancelled") throw failure("OperationCancelled");
    // At most one question of each kind belongs to an operation. Retain private
    // question identity so duplicate callbacks join/replay without another UI.
    const fingerprint =
      request.tag === "Calling"
        ? `${request.value.network.join(",")}:${request.value.account.join(",")}`
        : request.tag;
    const previous = existing?.consents[request.tag];
    if (previous) {
      if (previous.fingerprint !== fingerprint)
        throw failure("OperationConflict");
      return previous.promise;
    }
    const op = operation(rt, id);
    const controller = new win.AbortController();
    const question: MediaConsentRequest =
      request.tag === "Calling"
        ? {
            tag: "Calling",
            value: {
              network: request.value.network.slice(),
              account: request.value.account.slice(),
            },
          }
        : { tag: request.tag };
    const context: BrowserMediaConsentContext = Object.freeze({
      productId,
      runtimeId: rt.id,
      operationId: id,
      signal: controller.signal,
    });
    op.status = "consenting";
    const consent: Consent = {
      fingerprint,
      controller,
      promise: Promise.resolve()
        .then(async (): Promise<MediaBackendResponse> => {
          if (
            disposed ||
            rt.closed ||
            op.status !== "consenting" ||
            controller.signal.aborted
          )
            throw failure("OperationCancelled");
          const granted = await Promise.race([
            options.requestConsent(question, context),
            op.cancelled,
          ]);
          if (
            disposed ||
            rt.closed ||
            op.status !== "consenting" ||
            controller.signal.aborted
          )
            throw failure("OperationCancelled");
          if (typeof granted !== "boolean") throw failure();
          op.status = granted ? "admitted" : "failed";
          return { tag: "Consent", value: { granted } };
        })
        .catch((error) => {
          const cancelled =
            controller.signal.aborted || op.status === "cancelled";
          if (!cancelled) op.status = "failed";
          controller.abort();
          if (
            cancelled ||
            ((error instanceof Error || error instanceof win.DOMException) &&
              error.name === "AbortError")
          )
            throw failure("OperationCancelled");
          throw error instanceof CommandRejected ? error : failure();
        })
        .finally(() => {
          consent.controller = undefined;
          if (op.activeConsent === consent) op.activeConsent = undefined;
        }),
    };
    op.consents[request.tag] = consent;
    op.activeConsent = consent;
    return consent.promise;
  }
  function operationAlive(op: Operation): boolean {
    return (
      op.status === "preparing" &&
      !!op.session &&
      alive(op.session) &&
      op.session.pending === op
    );
  }
  function collectCapture(
    op: Operation,
    promise: Promise<MediaStream>,
    kinds: readonly TrackKind[],
  ): Promise<void> {
    const capture = promise.then((stream) => {
      const obtained = stream.getTracks();
      if (!operationAlive(op)) {
        for (const track of obtained) track.stop();
        throw failure("OperationCancelled");
      }
      for (const track of obtained) track.enabled = false;
      const audio = stream.getAudioTracks();
      const video = stream.getVideoTracks();
      for (const kind of kinds) {
        const track = kind === "microphone" ? audio.shift() : video.shift();
        if (!track || track.readyState !== "live") {
          for (const captured of obtained) captured.stop();
          throw failure("DeviceUnavailable");
        }
        op.tracks[kind] = track;
      }
      for (const extra of [...audio, ...video]) extra.stop();
    });
    return Promise.race([capture, op.cancelled]);
  }
  function chooseScreen(op: Operation): Promise<void> {
    const devices = win.navigator.mediaDevices;
    if (typeof devices.getDisplayMedia !== "function") {
      return Promise.reject(failure("DeviceUnavailable"));
    }
    const session = op.session!;
    // Browser screen pickers require activation in this host realm. Never rely
    // on activation surviving async core consent or an iframe postMessage.
    const button = html("button");
    button.type = "button";
    button.textContent = "Choose screen to share";
    op.pickerButton = button;
    session.indicator.append(button);
    const picked = new Promise<void>((resolve, reject) => {
      button.addEventListener(
        "click",
        () => {
          if (!operationAlive(op)) return;
          button.disabled = true;
          let captured: Promise<MediaStream>;
          try {
            captured = devices.getDisplayMedia(
              webIdl<DisplayMediaStreamOptions>({ video: true, audio: false }),
            );
          } catch {
            reject(failure("CaptureCancelled"));
            return;
          }
          collectCapture(op, captured, ["screen"]).then(resolve, () =>
            reject(failure("CaptureCancelled")),
          );
        },
        webIdl<AddEventListenerOptions>({ once: true }),
      );
    });
    return Promise.race([picked, op.cancelled]).finally(() => button.remove());
  }
  async function prepare(op: Operation): Promise<MediaBackendResponse> {
    const session = op.session!;
    const intent = op.requested!;
    try {
      const need: TrackKind[] = [];
      for (const kind of TRACK_KINDS) {
        if (!intent[kind]) continue;
        const current = session.tracks[kind];
        const cameraChanged =
          kind === "camera" &&
          intent.cameraPreference !== session.requested.cameraPreference;
        if (current?.readyState === "live" && !cameraChanged) {
          // Independent ownership: later commits may stop the old original.
          const clone = current.clone();
          clone.enabled = false;
          op.tracks[kind] = clone;
        } else need.push(kind);
      }
      const jobs: Promise<void>[] = [];
      const captureKinds = need.filter((kind) => kind !== "screen");
      if (captureKinds.length) {
        const facing =
          intent.cameraPreference === "Front"
            ? "user"
            : intent.cameraPreference === "Rear"
              ? "environment"
              : undefined;
        jobs.push(
          collectCapture(
            op,
            win.navigator.mediaDevices.getUserMedia(
              webIdl<MediaStreamConstraints>({
                audio: need.includes("microphone")
                  ? {
                      echoCancellation: { exact: true },
                      noiseSuppression: true,
                    }
                  : false,
                video: need.includes("camera")
                  ? { facingMode: facing ? { ideal: facing } : undefined }
                  : false,
              }),
            ),
            captureKinds,
          ),
        );
      }
      if (need.includes("screen")) jobs.push(chooseScreen(op));
      await Promise.race([Promise.all(jobs), op.cancelled]);
      if (!operationAlive(op)) throw failure("OperationCancelled");
      op.status = "prepared";
      return { tag: "Done" };
    } catch (error) {
      if (op.status !== "cancelled") op.status = "failed";
      stopTracks(op.tracks);
      op.tracks = {};
      op.pickerButton?.remove();
      if (session.pending === op) session.pending = undefined;
      if (op.opening) closeSession(session);
      else updateIndicator(session);
      if (op.status === "cancelled") throw failure("OperationCancelled");
      if (error instanceof CommandRejected) throw error;
      const name = error instanceof win.DOMException ? error.name : "";
      throw failure(
        name === "NotAllowedError" || name === "SecurityError"
          ? "Denied"
          : name === "NotFoundError" ||
              name === "NotReadableError" ||
              name === "OverconstrainedError"
            ? "DeviceUnavailable"
            : name === "AbortError"
              ? "CaptureCancelled"
              : "CaptureFailed",
      );
    }
  }

  function createSession(rt: Runtime, id: SessionId): Session {
    if (rt.sessions.has(id)) throw failure("InvalidHandle");
    if (
      rt.sessions.size >= MAX_ISSUED_SESSIONS ||
      [...rt.sessions.values()].filter((session) => !session.closed).length >=
        capabilities.maxSessions
    ) {
      throw domainFailure({
        tag: "ResourceExhausted",
        value: { resource: "Sessions" },
      });
    }
    const indicator = html("div");
    const label = html("span");
    const hangup = html("button");
    const stopScreen = html("button");
    const resumeAudio = html("button");
    indicator.setAttribute("role", "group");
    indicator.setAttribute("aria-label", "Host media controls");
    label.setAttribute("aria-live", "polite");
    for (const button of [hangup, stopScreen, resumeAudio])
      button.type = "button";
    hangup.textContent = "End call";
    stopScreen.textContent = "Stop sharing screen";
    resumeAudio.textContent = "Enable call audio";
    indicator.append(label, hangup, stopScreen, resumeAudio);
    options.indicatorMount.append(indicator);
    const session: Session = {
      runtime: rt,
      id,
      key: id,
      closed: false,
      committed: false,
      requested: emptyIntent(),
      tracks: {},
      revision: 0n,
      admittedRevision: 0n,
      peers: new Map(),
      serial: Promise.resolve(),
      surfaces: [],
      indicator,
      label,
      stopScreen,
      resumeAudio,
      routeGeneration: 0,
      routeSerial: Promise.resolve(),
      listeners: [],
    };
    rt.sessions.set(session.key, session);
    hangup.addEventListener("click", () => {
      if (!alive(session)) return;
      closeSession(session);
      emit(rt, { tag: "HostEnded", value: { sessionId: id } });
    });
    stopScreen.addEventListener("click", () => hostStopScreen(session));
    resumeAudio.addEventListener("click", () => {
      for (const peer of session.peers.values())
        if (peerAlive(peer)) void playAudio(peer);
    });
    updateIndicator(session);
    return session;
  }
  function watchLocal(session: Session): void {
    for (const remove of session.listeners.splice(0)) remove();
    for (const kind of TRACK_KINDS) {
      const track = session.tracks[kind];
      if (!track) continue;
      const changed = () => {
        if (!alive(session) || session.tracks[kind] !== track) return;
        if (kind === "screen" && track.readyState === "ended")
          hostStopScreen(session);
        else publishLocal(session);
      };
      for (const event of ["mute", "unmute", "ended"]) {
        track.addEventListener(event, changed);
        session.listeners.push(() => track.removeEventListener(event, changed));
      }
    }
  }
  function hostStopScreen(session: Session): void {
    if (!alive(session)) return;
    if (session.pending) cancelOperation(session.pending);
    // Host stop is stronger than an in-flight sender swap. Stop both old and
    // prepared screen tracks synchronously so a late replaceTrack completion
    // cannot resume sharing after the user pressed this trusted control.
    for (const op of session.runtime.operations.values()) {
      if (op.session !== session || op.status !== "committing") continue;
      op.tracks.screen?.stop();
      delete op.tracks.screen;
      if (op.requested) op.requested = { ...op.requested, screen: false };
    }
    const track = session.tracks.screen;
    if (!track) return;
    track.enabled = false;
    track.stop();
    delete session.tracks.screen;
    session.requested = { ...session.requested, screen: false };
    for (const peer of session.peers.values()) {
      const slot = peer.slots[2];
      if (!peerAlive(peer) || !slot) continue;
      slot.direction = "recvonly";
      void slot.sender.replaceTrack(null).catch(() => failPeer(peer));
    }
    publishLocal(session);
    emit(session.runtime, {
      tag: "ScreenStopped",
      value: { sessionId: session.id },
    });
  }
  async function commit(op: Operation): Promise<MediaBackendResponse> {
    if (op.status === "committed") return op.result!;
    if (op.status === "committing") return op.promise!;
    if (op.status !== "prepared" || !op.session)
      throw failure("OperationCancelled");
    const session = op.session;
    op.status = "committing";
    op.promise = serializeSession(session, async () => {
      const previous = session.tracks;
      if (
        Object.values(op.tracks).some((track) => track?.readyState !== "live")
      )
        throw failure("DeviceUnavailable");
      for (const track of Object.values(previous))
        if (track) track.enabled = false;
      try {
        const peers = [...session.peers.values()].filter(
          (peer) => peerAlive(peer) && peer.slots.length === TRACK_KINDS.length,
        );
        await Promise.all(
          peers.map(async (peer) => {
            if (!peerAlive(peer)) return;
            await Promise.all(
              TRACK_KINDS.map((kind, index) =>
                peer.slots[index].sender.replaceTrack(op.tracks[kind] ?? null),
              ),
            );
          }),
        );
        requireAlive(session);
        // Tracks stay disabled until every live sender has completed its swap.
        // No prepared capture can be transmitted before this commit boundary.
        session.tracks = op.tracks;
        op.tracks = {};
        session.requested = { ...op.requested! };
        session.revision = op.revision;
        session.committed = true;
        if (session.pending === op) session.pending = undefined;
        for (const peer of peers) {
          if (!peerAlive(peer)) continue;
          TRACK_KINDS.forEach((kind, index) => {
            peer.slots[index].direction = session.tracks[kind]
              ? "sendrecv"
              : "recvonly";
          });
        }
        for (const track of Object.values(session.tracks))
          if (track) track.enabled = true;
        stopTracks(previous);
        watchLocal(session);
        op.status = "committed";
        op.result = {
          tag: "LocalState",
          value: { state: localState(session) },
        };
        publishLocal(session);
        void updateAudioRoute(session);
        return op.result;
      } catch {
        stopTracks(op.tracks);
        op.tracks = {};
        // A failed sender swap must not leave half the peers using new media.
        for (const peer of session.peers.values()) {
          if (!peerAlive(peer)) continue;
          try {
            await Promise.all(
              TRACK_KINDS.map((kind, index) =>
                peer.slots[index]?.sender.replaceTrack(previous[kind] ?? null),
              ),
            );
          } catch {
            failPeer(peer);
          }
        }
        if (alive(session))
          for (const track of Object.values(previous))
            if (track?.readyState === "live") track.enabled = true;
        throw failure();
      }
    }).catch((error) => {
      op.status = "failed";
      stopTracks(op.tracks);
      op.tracks = {};
      if (session.pending === op) session.pending = undefined;
      if (op.opening) closeSession(session);
      else updateIndicator(session);
      throw error instanceof CommandRejected ? error : failure();
    });
    return op.promise;
  }

  async function playAudio(peer: Peer): Promise<void> {
    if (!peerAlive(peer) || !peer.remote.microphone) return;
    try {
      await peer.audio.play();
      if (!peerAlive(peer)) {
        peer.audio.pause();
        return;
      }
      peer.playbackBlocked = false;
    } catch {
      if (!peerAlive(peer)) return;
      peer.playbackBlocked = true;
    }
    updateIndicator(peer.session);
    publishRemote(peer);
  }
  async function updateAudioRoute(session: Session): Promise<void> {
    const generation = ++session.routeGeneration;
    if (!options.resolveAudioOutput) return;
    try {
      const output = await options.resolveAudioOutput(
        session.requested.audioPreference,
      );
      if (!alive(session) || generation !== session.routeGeneration || !output)
        return;
      // Serialize actual sink mutations, but not resolver/consent work. An older
      // setSinkId completion can never overwrite a newer accepted route.
      const applied = session.routeSerial.then(async () => {
        if (!alive(session) || generation !== session.routeGeneration) return;
        const peers = [...session.peers.values()].filter(peerAlive);
        if (!peers.length) return;
        for (const peer of peers) {
          const audio = peer.audio as HTMLAudioElement & {
            setSinkId?: (id: string) => Promise<void>;
            sinkId?: string;
          };
          if (!audio.setSinkId) return; // OS-managed default route, no invented class.
          await audio.setSinkId(output.sinkId);
          if (!peerAlive(peer) || generation !== session.routeGeneration)
            return;
          if (audio.sinkId !== output.sinkId) return;
        }
        session.route = output.route;
        publishLocal(session);
      });
      session.routeSerial = applied.catch(() => undefined);
      await applied;
    } catch {
      if (alive(session) && generation === session.routeGeneration) {
        session.route = undefined;
        publishLocal(session);
      }
    }
  }

  function publishPeer(peer: Peer, state: MediaBackendPeerState): void {
    if (!peerAlive(peer) || state === peer.state) return;
    peer.state = state;
    emit(peer.session.runtime, {
      tag: "PeerStateChanged",
      value: { sessionId: peer.session.id, participantId: peer.id, state },
    });
  }
  function publishRemote(peer: Peer): void {
    if (!peerAlive(peer)) return;
    const states = {} as MediaRemoteState;
    TRACK_KINDS.forEach((kind, index) => {
      const direction = peer.slots[index]?.currentDirection;
      const receiving = direction === "recvonly" || direction === "sendrecv";
      const track = peer.remote[kind];
      states[kind] =
        !receiving || !track
          ? "Off"
          : track.readyState === "ended"
            ? "Interrupted"
            : track.muted
              ? peer.seen.has(kind)
                ? "Interrupted"
                : "Starting"
              : kind === "microphone" && peer.playbackBlocked
                ? "Interrupted"
                : "Live";
    });
    peer.session.runtime.dirty = true;
    scheduleFrame();
    emit(peer.session.runtime, {
      tag: "RemoteStateChanged",
      value: {
        sessionId: peer.session.id,
        participantId: peer.id,
        state: states,
      },
    });
  }
  function deadline(peer: Peer): void {
    if (!peerAlive(peer) || peer.timer !== undefined) return;
    peer.timer = win.setTimeout(() => failPeer(peer), CONNECTION_TIMEOUT);
  }
  function failPeer(peer: Peer): void {
    if (!peerAlive(peer)) return;
    publishPeer(peer, "Failed");
    closePeer(peer);
  }
  function serializePeer<T>(peer: Peer, action: () => Promise<T>): Promise<T> {
    if (peer.pendingCommands >= MAX_ICE_CANDIDATES) {
      failPeer(peer);
      return Promise.reject(failure("EventOverflow"));
    }
    peer.pendingCommands += 1;
    const next = peer.serial
      .then(async () => {
        await peer.ready;
        if (!peerAlive(peer)) throw failure("InvalidHandle");
        return action();
      })
      .finally(() => {
        peer.pendingCommands -= 1;
      });
    peer.serial = next.catch(() => undefined);
    return next;
  }
  function emitDescription(peer: Peer): void {
    if (!peerAlive(peer)) return;
    const description = peer.pc?.localDescription;
    if (
      !description ||
      (description.type !== "offer" && description.type !== "answer")
    )
      return;
    emit(peer.session.runtime, {
      tag: "Description",
      value: {
        sessionId: peer.session.id,
        participantId: peer.id,
        description: {
          kind: description.type === "offer" ? "Offer" : "Answer",
          // Descriptions created after gathering embed candidates; keep relays only.
          sdp: description.sdp
            .split("\r\n")
            .filter(
              (line) =>
                !line.startsWith("a=candidate:") || RELAY_CANDIDATE.test(line),
            )
            .join("\r\n"),
        },
      },
    });
  }
  function negotiate(peer: Peer): void {
    void serializePeer(peer, async () => {
      const pc = peer.pc!;
      if (
        pc.signalingState !== "stable" ||
        (!peer.offerer && !peer.remoteOfferSeen)
      )
        return;
      peer.makingOffer = true;
      try {
        const offer = await pc.createOffer();
        if (!peerAlive(peer) || pc.signalingState !== "stable") return;
        await pc.setLocalDescription(
          webIdl<RTCSessionDescriptionInit>({
            type: offer.type,
            sdp: offer.sdp,
          }),
        );
        emitDescription(peer);
      } finally {
        peer.makingOffer = false;
      }
    }).catch(() => {
      if (peerAlive(peer)) failPeer(peer);
    });
  }
  function adoptRemoteSlots(peer: Peer): void {
    if (peer.slots.length) return;
    const slots = peer.pc!.getTransceivers();
    if (
      slots.length !== TRACK_KINDS.length ||
      slots.some(
        (slot, index) =>
          slot.mid === null ||
          slot.receiver.track.kind !== (index === 0 ? "audio" : "video"),
      )
    ) {
      failPeer(peer);
      throw failure("InvalidDescription");
    }
    peer.slots = slots;
  }
  async function createPeer(
    session: Session,
    id: ParticipantId,
    offerer: boolean,
  ): Promise<void> {
    if (!session.committed) throw failure("InvalidState");
    const existing = session.peers.get(id);
    if (existing) {
      if (existing.closed || existing.offerer !== offerer)
        throw failure("InvalidHandle");
      await existing.ready;
      return;
    }
    if (
      session.peers.size >= MAX_ISSUED_PEERS ||
      [...session.peers.values()].filter((peer) => !peer.closed).length >=
        capabilities.maxRemoteParticipants
    ) {
      throw domainFailure({
        tag: "CapacityExceeded",
        value: { limit: capabilities.maxRemoteParticipants },
      });
    }
    const audio = html("audio");
    audio.autoplay = true;
    audio.hidden = true;
    session.indicator.append(audio);
    const peer: Peer = {
      id,
      key: id,
      session,
      closed: false,
      serial: Promise.resolve(),
      pendingCommands: 0,
      slots: [],
      remote: {},
      seen: new Set(),
      candidates: [],
      offerer,
      makingOffer: false,
      ignoreOffer: false,
      remoteOfferSeen: false,
      state: "Connecting",
      audio,
      playbackBlocked: false,
    };
    session.peers.set(peer.key, peer);
    deadline(peer);
    emit(session.runtime, {
      tag: "PeerStateChanged",
      value: { sessionId: session.id, participantId: id, state: "Connecting" },
    });
    peer.ready = Promise.resolve()
      .then(() =>
        typeof options.iceServers === "function"
          ? options.iceServers(session.runtime.id)
          : options.iceServers,
      )
      .then((iceServers) =>
        serializeSession(session, async () => {
          if (!peerAlive(peer)) throw failure("SessionEnded");
          // Relay-only is not negotiable: host or reflexive candidates would
          // reveal this device's addresses to every remote participant.
          const pc = new win.RTCPeerConnection(
            webIdl<RTCConfiguration>({
              iceServers: [...iceServers],
              iceTransportPolicy: "relay",
            }),
          );
          peer.pc = pc;
          // Only the offerer creates slots. An answerer's addTransceiver() slots
          // are not reused by setRemoteDescription(), which would create extras.
          // The answerer adopts the incoming fixed audio/camera/screen m-line order.
          if (offerer) {
            peer.slots = TRACK_KINDS.map((kind) =>
              pc.addTransceiver(
                session.tracks[kind] ??
                  (kind === "microphone" ? "audio" : "video"),
                webIdl<RTCRtpTransceiverInit>({
                  direction: session.tracks[kind] ? "sendrecv" : "recvonly",
                }),
              ),
            );
          }
          pc.onicecandidate = (event) => {
            if (!peerAlive(peer) || !event.candidate) return;
            const candidate = event.candidate;
            if (
              !RELAY_CANDIDATE.test(candidate.candidate) ||
              (candidate.type ?? "relay") !== "relay"
            )
              return;
            emit(session.runtime, {
              tag: "IceCandidate",
              value: {
                sessionId: session.id,
                participantId: id,
                candidate: {
                  candidate: candidate.candidate,
                  mid: candidate.sdpMid ?? undefined,
                  mlineIndex: candidate.sdpMLineIndex ?? undefined,
                },
              },
            });
          };
          pc.onnegotiationneeded = () => {
            if (peerAlive(peer)) negotiate(peer);
          };
          pc.onconnectionstatechange = () => {
            if (!peerAlive(peer)) return;
            if (pc.connectionState === "connected") {
              if (peer.timer !== undefined) win.clearTimeout(peer.timer);
              peer.timer = undefined;
              publishPeer(peer, "Connected");
            } else if (pc.connectionState === "disconnected") {
              publishPeer(peer, "Reconnecting");
              deadline(peer);
              if (peer.offerer && typeof pc.restartIce === "function")
                pc.restartIce();
            } else if (
              pc.connectionState === "failed" ||
              pc.connectionState === "closed"
            )
              failPeer(peer);
          };
          pc.ontrack = (event) => {
            if (!peerAlive(peer)) {
              event.track.stop();
              return;
            }
            // Track events fire before setRemoteDescription() resolves.
            try {
              adoptRemoteSlots(peer);
            } catch {
              event.track.stop();
              return;
            }
            const index = peer.slots.indexOf(event.transceiver);
            const kind = TRACK_KINDS[index];
            if (
              !kind ||
              event.track.kind !== (kind === "microphone" ? "audio" : "video")
            ) {
              event.track.stop();
              failPeer(peer);
              return;
            }
            const track = event.track;
            // Reactivating an RTP receiver fires ontrack again with the same track.
            // Keep its existing observers and playback binding; stopping it is final.
            if (peer.remote[kind] === track) return;
            peer.remote[kind]?.stop();
            peer.remote[kind] = track;
            const changed = () => {
              if (!peerAlive(peer) || peer.remote[kind] !== track) return;
              if (!track.muted && track.readyState === "live")
                peer.seen.add(kind);
              publishRemote(peer);
            };
            track.onmute = changed;
            track.onunmute = changed;
            track.onended = changed;
            if (kind === "microphone") {
              audio.srcObject = trackStream(track);
              void playAudio(peer);
            }
            changed();
          };
          if (offerer) negotiate(peer);
          void updateAudioRoute(session);
        }),
      )
      .catch(() => {
        failPeer(peer);
        throw failure();
      });
    await peer.ready;
  }
  async function applyDescription(
    peer: Peer,
    description: Extract<
      MediaBackendCommand,
      { tag: "ApplyDescription" }
    >["value"]["description"],
  ): Promise<void> {
    if (description.sdp.length > 262144) throw failure("InvalidDescription");
    const media = description.sdp
      .split(/\r?\n/)
      .filter((line) => line.startsWith("m="))
      .map((line) => line.split(" ")[0]);
    if (
      media.length !== 3 ||
      media[0] !== "m=audio" ||
      media[1] !== "m=video" ||
      media[2] !== "m=video"
    )
      throw failure("InvalidDescription");
    await serializePeer(peer, () =>
      serializeSession(peer.session, async () => {
        const pc = peer.pc!;
        const offer = description.kind === "Offer";
        const collision =
          offer && (peer.makingOffer || pc.signalingState !== "stable");
        peer.ignoreOffer = peer.offerer && collision;
        if (peer.ignoreOffer) return;
        if (collision)
          await pc.setLocalDescription(
            webIdl<RTCSessionDescriptionInit>({ type: "rollback" }),
          );
        if (!peerAlive(peer)) return;
        await pc.setRemoteDescription(
          webIdl<RTCSessionDescriptionInit>({
            type: offer ? "offer" : "answer",
            sdp: description.sdp,
          }),
        );
        if (!peerAlive(peer)) return;
        adoptRemoteSlots(peer);
        // Serialize initial sender binding with local track commits. Even a
        // receive-only offer has slots, despite producing no incoming track event.
        if (offer) {
          await Promise.all(
            TRACK_KINDS.map((kind, index) =>
              peer.slots[index].sender.replaceTrack(
                peer.session.tracks[kind] ?? null,
              ),
            ),
          );
          if (!peerAlive(peer)) return;
          TRACK_KINDS.forEach((kind, index) => {
            peer.slots[index].direction = peer.session.tracks[kind]
              ? "sendrecv"
              : "recvonly";
          });
        }
        peer.remoteOfferSeen = true;
        for (const candidate of peer.candidates.splice(0)) {
          await pc.addIceCandidate(webIdl(candidate));
          if (!peerAlive(peer)) return;
        }
        if (offer) {
          const answer = await pc.createAnswer();
          if (!peerAlive(peer)) return;
          await pc.setLocalDescription(
            webIdl<RTCSessionDescriptionInit>({
              type: answer.type,
              sdp: answer.sdp,
            }),
          );
          emitDescription(peer);
        }
        publishRemote(peer);
      }),
    );
  }
  async function addCandidate(
    peer: Peer,
    candidate: Extract<
      MediaBackendCommand,
      { tag: "AddIceCandidate" }
    >["value"]["candidate"],
  ): Promise<void> {
    if (candidate.candidate.length > 8192 || (candidate.mid?.length ?? 0) > 256)
      throw failure("InvalidCandidate");
    await serializePeer(peer, async () => {
      if (peer.ignoreOffer) return;
      const value: RTCIceCandidateInit = {
        candidate: candidate.candidate,
        sdpMid: candidate.mid,
        sdpMLineIndex: candidate.mlineIndex,
      };
      if (!peer.pc!.remoteDescription) {
        if (peer.candidates.length >= MAX_ICE_CANDIDATES) {
          failPeer(peer);
          throw failure("EventOverflow");
        }
        peer.candidates.push(value);
      } else await peer.pc!.addIceCandidate(webIdl(value));
    });
  }
  function requirePeer(session: Session, id: ParticipantId): Peer {
    const peer = session.peers.get(id);
    if (!peer || !peerAlive(peer)) throw failure("InvalidHandle");
    return peer;
  }
  function closePeer(peer: Peer): void {
    if (peer.closed) return;
    peer.closed = true;
    if (peer.timer !== undefined) win.clearTimeout(peer.timer);
    if (peer.pc) {
      peer.pc.onicecandidate = null;
      peer.pc.ontrack = null;
      peer.pc.onconnectionstatechange = null;
      peer.pc.onnegotiationneeded = null;
      peer.pc.close();
    }
    stopTracks(peer.remote);
    peer.remote = {};
    peer.candidates.length = 0;
    peer.audio.pause();
    peer.audio.srcObject = null;
    peer.audio.remove();
    peer.session.runtime.dirty = true;
    scheduleFrame();
    updateIndicator(peer.session);
  }

  function releasePlane(plane: Plane): void {
    plane.video.pause();
    plane.video.srcObject = null;
    plane.wrapper.remove();
  }
  /** Leave `wanted` as the container's children, in order, moving nothing already in place. */
  function arrangePlanes(container: Element, wanted: HTMLElement[]): void {
    let cursor = container.firstChild;
    for (const node of wanted) {
      if (node === cursor) cursor = cursor.nextSibling;
      else container.insertBefore(node, cursor);
    }
    while (cursor) {
      const next = cursor.nextSibling;
      cursor.remove();
      cursor = next;
    }
  }
  function clearLayouts(rt: Runtime): void {
    for (const session of rt.sessions.values()) {
      session.surfaces = [];
      session.queued = undefined;
      // Layout counters never restart, even when attachments change.
    }
    if (rt.attachment) {
      for (const plane of rt.attachment.planes.values()) releasePlane(plane);
      rt.attachment.planes.clear();
      rt.attachment.below.replaceChildren();
      rt.attachment.above.replaceChildren();
    }
    rt.dirty = false;
  }
  function closeSession(session: Session): void {
    if (session.closed) return;
    session.closed = true;
    for (const op of session.runtime.operations.values()) {
      if (op.session !== session) continue;
      if (op.status === "committing") {
        stopTracks(op.tracks);
        op.tracks = {};
      } else cancelOperation(op);
    }
    session.pending = undefined;
    for (const peer of session.peers.values()) closePeer(peer);
    for (const remove of session.listeners.splice(0)) remove();
    stopTracks(session.tracks);
    session.tracks = {};
    session.surfaces = [];
    session.queued = undefined;
    session.indicator.remove();
    session.runtime.dirty = true;
    // Teardown clears decoded pictures immediately, without waiting for a frame.
    render(session.runtime);
  }
  function detach(rt: Runtime, notify = true): void {
    clearLayouts(rt);
    rt.attachment?.below.remove();
    rt.attachment?.above.remove();
    rt.attachment = undefined;
    rt.viewport = undefined;
    rt.viewportRevision += 1n;
    if (notify)
      emit(rt, { tag: "ViewportChanged", value: { viewport: undefined } });
  }
  function closeRuntime(rt: Runtime, reason?: string): void {
    if (rt.closed) return;
    rt.closed = true;
    for (const op of rt.operations.values()) cancelOperation(op);
    for (const session of rt.sessions.values()) closeSession(session);
    detach(rt, false);
    for (const queue of rt.queues) finishQueue(queue, reason);
    rt.queues.clear();
    rt.pendingEvents.length = 0;
  }

  function defaultGeometry(
    product: Element,
    mount: Element,
  ): BrowserMediaGeometry | undefined {
    // Native/XUL embeddings without HTML layout metrics supply measureViewport.
    const productLayout = product as HTMLElement;
    const mountLayout = mount as HTMLElement;
    const bounds = product.getBoundingClientRect();
    const base = mount.getBoundingClientRect();
    const width = product.clientWidth;
    const height = product.clientHeight;
    if (
      !width ||
      !height ||
      !mountLayout.offsetWidth ||
      !mountLayout.offsetHeight ||
      !productLayout.offsetWidth ||
      !productLayout.offsetHeight
    )
      return undefined;
    const mountScale = base.width / mountLayout.offsetWidth;
    const productScale = bounds.width / productLayout.offsetWidth;
    if (
      !(mountScale > 0) ||
      !(productScale > 0) ||
      Math.abs(base.height / mountLayout.offsetHeight - mountScale) > 0.001 ||
      Math.abs(bounds.height / productLayout.offsetHeight - productScale) >
        0.001
    )
      return undefined;
    const x = bounds.left + product.clientLeft * productScale;
    const y = bounds.top + product.clientTop * productScale;
    const visual = win.visualViewport;
    let visible = {
      x: visual?.offsetLeft ?? 0,
      y: visual?.offsetTop ?? 0,
      width: visual?.width ?? win.innerWidth,
      height: visual?.height ?? win.innerHeight,
    };
    for (
      let element: Element | null = product;
      element;
      element = element.parentElement
    ) {
      const style = win.getComputedStyle(element);
      if (
        style.display === "none" ||
        style.visibility !== "visible" ||
        Number(style.opacity) === 0 ||
        style.contentVisibility === "hidden"
      )
        return undefined;
      if (style.transform !== "none") {
        const matrix = new win.DOMMatrixReadOnly(style.transform);
        if (
          !matrix.is2D ||
          matrix.b !== 0 ||
          matrix.c !== 0 ||
          matrix.a <= 0 ||
          matrix.d <= 0 ||
          Math.abs(matrix.a - matrix.d) > 0.001
        )
          return undefined;
      }
      if (
        element !== product &&
        (style.overflowX !== "visible" || style.overflowY !== "visible")
      ) {
        const rect = element.getBoundingClientRect();
        visible = intersect(visible, {
          x: style.overflowX === "visible" ? visible.x : rect.left,
          y: style.overflowY === "visible" ? visible.y : rect.top,
          width: style.overflowX === "visible" ? visible.width : rect.width,
          height: style.overflowY === "visible" ? visible.height : rect.height,
        });
      }
    }
    return {
      left: (x - base.left) / mountScale - mount.clientLeft + mount.scrollLeft,
      top: (y - base.top) / mountScale - mount.clientTop + mount.scrollTop,
      width,
      height,
      scale: productScale / mountScale,
      deviceScale: win.devicePixelRatio * (visual?.scale ?? 1) * productScale,
      clip: intersect(
        { x: 0, y: 0, width, height },
        {
          x: (visible.x - x) / productScale,
          y: (visible.y - y) / productScale,
          width: visible.width / productScale,
          height: visible.height / productScale,
        },
      ),
      visible: true,
    };
  }
  function geometryValid(geometry: BrowserMediaGeometry): boolean {
    return (
      [
        geometry.left,
        geometry.top,
        geometry.width,
        geometry.height,
        geometry.scale,
        geometry.deviceScale,
        geometry.clip.x,
        geometry.clip.y,
        geometry.clip.width,
        geometry.clip.height,
      ].every(Number.isFinite) &&
      Number.isInteger(geometry.width) &&
      Number.isInteger(geometry.height) &&
      geometry.width > 0 &&
      geometry.height > 0 &&
      geometry.width <= 4294967295 &&
      geometry.height <= 4294967295 &&
      geometry.scale > 0 &&
      Math.round(geometry.deviceScale * 1_000_000) > 0 &&
      geometry.deviceScale <= 1000 &&
      geometry.clip.width >= 0 &&
      geometry.clip.height >= 0
    );
  }
  function attachmentIsolated(product: Element, runtimeId: bigint): boolean {
    try {
      if (!options.isProductIsolated(product, runtimeId)) return false;
      if (product.namespaceURI === HTML_NS && product.localName === "iframe") {
        const frame = product as HTMLIFrameElement;
        if (
          !frame.contentWindow ||
          !frame.hasAttribute("sandbox") ||
          frame.allowFullscreen
        )
          return false;
        const policies = frame.allow
          .split(";")
          .map((policy) => policy.trim().replace(/\s+/g, " "));
        if (
          ![
            "microphone 'none'",
            "camera 'none'",
            "display-capture 'none'",
            "fullscreen 'none'",
          ].every((policy) => policies.includes(policy))
        )
          return false;
        // A host assertion cannot waive the actual same-origin boundary. A
        // readable child document can also reach its parent and our raw tracks.
        // contentDocument is null exactly when the frame's current document is
        // not same-origin (including opaque sandbox origins); the initial
        // about:blank is readable, so an unloaded frame fails closed. The value
        // is used, unlike a read probed for its exception, which minifiers that
        // assume side-effect-free property reads delete.
        return frame.contentDocument === null;
      }
      if (
        product.namespaceURI ===
          "http://www.mozilla.org/keymaster/gatekeeper/there.is.only.xul" &&
        product.localName === "browser"
      ) {
        type Principal = { isSystemPrincipal: boolean };
        const host = doc as Document & { nodePrincipal?: Principal };
        const browser = product as Element & {
          contentPrincipal?: Principal;
          isRemoteBrowser?: boolean;
        };
        return (
          host.nodePrincipal?.isSystemPrincipal === true &&
          browser.isRemoteBrowser === true &&
          browser.contentPrincipal?.isSystemPrincipal === false
        );
      }
      return false;
    } catch {
      return false;
    }
  }
  function refreshViewport(rt: Runtime): void {
    const attachment = rt.attachment;
    if (!attachment || rt.closed) return;
    if (
      options.getProductElement(rt.id) !== attachment.product ||
      options.getCompositorMount(rt.id) !== attachment.mount ||
      !attachment.product.isConnected ||
      !attachment.mount.isConnected ||
      attachment.product.parentElement !== attachment.mount ||
      !attachmentIsolated(attachment.product, rt.id)
    ) {
      detach(rt);
      return;
    }
    let geometry: BrowserMediaGeometry | undefined;
    try {
      geometry = options.measureViewport
        ? options.measureViewport(attachment.product, attachment.mount, rt.id)
        : defaultGeometry(attachment.product, attachment.mount);
    } catch {
      geometry = undefined;
    }
    if (
      doc.visibilityState === "hidden" ||
      !geometry ||
      !geometryValid(geometry) ||
      !geometry.visible ||
      !geometry.clip.width ||
      !geometry.clip.height
    )
      geometry = undefined;
    const signature = geometry ? JSON.stringify(geometry) : "hidden";
    if (signature === attachment.signature) return;
    attachment.signature = signature;
    attachment.geometry = geometry;
    clearLayouts(rt);
    rt.viewportRevision += 1n;
    rt.viewport = geometry
      ? {
          revision: rt.viewportRevision,
          width: geometry.width,
          height: geometry.height,
          deviceScaleNumerator: Math.round(geometry.deviceScale * 1_000_000),
          deviceScaleDenominator: 1_000_000,
        }
      : undefined;
    emit(rt, { tag: "ViewportChanged", value: { viewport: rt.viewport } });
  }
  function attach(rt: Runtime): void {
    detach(rt);
    const product = options.getProductElement(rt.id);
    const mount = options.getCompositorMount(rt.id);
    if (
      !product ||
      !mount ||
      product.ownerDocument !== doc ||
      mount.ownerDocument !== doc ||
      !product.isConnected ||
      !mount.isConnected ||
      product.parentElement !== mount ||
      !attachmentIsolated(product, rt.id) ||
      product.contains(options.indicatorMount) ||
      mount.contains(options.indicatorMount)
    )
      throw failure("SurfaceUnavailable");
    const productStyle = win.getComputedStyle(product);
    const mountStyle = win.getComputedStyle(mount);
    if (
      productStyle.position === "static" ||
      productStyle.zIndex !== "1" ||
      mountStyle.position === "static" ||
      mountStyle.isolation !== "isolate"
    ) {
      throw failure("SurfaceUnavailable");
    }
    const plane = (depth: number) => {
      const element = html("div");
      element.setAttribute("aria-hidden", "true");
      element.style.cssText = `position:absolute;inset:0;pointer-events:none;overflow:hidden;z-index:${depth};isolation:isolate;`;
      mount.append(element);
      return element;
    };
    rt.attachment = {
      product,
      mount,
      below: plane(0),
      above: plane(2),
      planes: new Map(),
      signature: "",
    };
    refreshViewport(rt);
    scheduleFrame();
  }
  function scheduleFrame(): void {
    if (
      disposed ||
      animationFrame !== undefined ||
      ![...runtimes.values()].some((rt) => !rt.closed && rt.attachment)
    )
      return;
    animationFrame = win.requestAnimationFrame(() => {
      animationFrame = undefined;
      for (const rt of runtimes.values()) {
        if (rt.closed || !rt.attachment) continue;
        refreshViewport(rt);
        if (rt.dirty) render(rt);
      }
      // Also detects CSS animations, embedding movement, scale/zoom and clipping
      // changes that ResizeObserver alone does not report.
      scheduleFrame();
    });
  }
  function surfaceTrack(
    session: Session,
    surface: MediaSurface,
  ): MediaStreamTrack | undefined {
    const kind =
      surface.source.value.picture === "Camera" ? "camera" : "screen";
    if (surface.source.tag === "Local") return session.tracks[kind];
    const peer = session.peers.get(surface.source.value.participantId);
    if (!peer || !peerAlive(peer)) return undefined;
    const direction = peer.slots[kind === "camera" ? 1 : 2]?.currentDirection;
    return direction === "recvonly" || direction === "sendrecv"
      ? peer.remote[kind]
      : undefined;
  }
  function render(rt: Runtime): void {
    const attachment = rt.attachment;
    const geometry = attachment?.geometry;
    if (!attachment || !geometry || !rt.viewport) return;
    const below: HTMLElement[] = [];
    const above: HTMLElement[] = [];
    const planes = new Map<string, Plane>();
    const entries: { session: Session; surface: MediaSurface }[] = [];
    for (const session of rt.sessions.values()) {
      if (session.closed) continue;
      if (session.queued) {
        session.surfaces = session.queued;
        session.queued = undefined;
      }
      for (const surface of session.surfaces)
        entries.push({ session, surface });
    }
    entries.sort(
      (a, b) =>
        a.surface.depth - b.surface.depth ||
        (a.session.key < b.session.key
          ? -1
          : a.session.key > b.session.key
            ? 1
            : 0) ||
        a.surface.surfaceId - b.surface.surfaceId,
    );
    for (const { session, surface } of entries) {
      const track = surfaceTrack(session, surface);
      if (
        !surface.visible ||
        !track ||
        trackState(track) !== "Live" ||
        !surface.rect.width ||
        !surface.rect.height
      )
        continue;
      const clip = intersect(surface.rect, surface.clip, geometry.clip, {
        x: 0,
        y: 0,
        width: geometry.width,
        height: geometry.height,
      });
      if (!clip.width || !clip.height) continue;
      const scale = geometry.deviceScale;
      // Map edges once to physical pixels. The outer clip rounds inward so a
      // fractional host/product clip can never reveal pixels outside its area.
      const left = Math.ceil(clip.x * scale) / scale;
      const top = Math.ceil(clip.y * scale) / scale;
      const right = Math.floor((clip.x + clip.width) * scale) / scale;
      const bottom = Math.floor((clip.y + clip.height) * scale) / scale;
      if (right <= left || bottom <= top) continue;
      const x = Math.floor(surface.rect.x * scale) / scale;
      const y = Math.floor(surface.rect.y * scale) / scale;
      const width =
        Math.ceil((surface.rect.x + surface.rect.width) * scale) / scale - x;
      const height =
        Math.ceil((surface.rect.y + surface.rect.height) * scale) / scale - y;
      const radius = Math.min(
        surface.cornerRadius,
        surface.rect.width / 2,
        surface.rect.height / 2,
      );
      const wrapperStyle = `position:absolute;pointer-events:none;overflow:hidden;left:${geometry.left + left * geometry.scale}px;top:${geometry.top + top * geometry.scale}px;width:${(right - left) * geometry.scale}px;height:${(bottom - top) * geometry.scale}px;`;
      const roundedStyle = `position:absolute;overflow:hidden;left:${(x - left) * geometry.scale}px;top:${(y - top) * geometry.scale}px;width:${width * geometry.scale}px;height:${height * geometry.scale}px;border-radius:${radius * geometry.scale}px;`;
      const videoStyle = `display:block;width:100%;height:100%;object-fit:${surface.fit === "Contain" ? "contain" : "cover"};transform:${surface.mirrored ? "scaleX(-1)" : "none"};pointer-events:none;`;
      // A picture whose track is unchanged keeps its <video>: a fresh element
      // shows nothing until its first decoded frame, which blanks a picture on
      // every layout a scrolling product submits.
      const key = `${surface.placement}:${session.key}:${surface.surfaceId}`;
      let plane = attachment.planes.get(key);
      if (
        !plane ||
        plane.track !== track ||
        !plane.video.srcObject ||
        planes.has(key)
      ) {
        const wrapper = html("div");
        const rounded = html("div");
        const video = html("video");
        video.muted = true;
        video.autoplay = true;
        video.playsInline = true;
        video.disablePictureInPicture = true;
        video.setAttribute("disableRemotePlayback", "");
        video.srcObject = trackStream(track);
        rounded.append(video);
        wrapper.append(rounded);
        plane = { wrapper, rounded, video, track };
        void video.play().catch(() => {
          video.srcObject = null;
        });
      }
      if (plane.wrapper.style.cssText !== wrapperStyle)
        plane.wrapper.style.cssText = wrapperStyle;
      if (plane.rounded.style.cssText !== roundedStyle)
        plane.rounded.style.cssText = roundedStyle;
      if (plane.video.style.cssText !== videoStyle)
        plane.video.style.cssText = videoStyle;
      planes.set(key, plane);
      (surface.placement === "BelowProduct" ? below : above).push(
        plane.wrapper,
      );
    }
    // Everything below runs synchronously within the same animation callback;
    // the browser cannot paint a partly replaced session set in between.
    for (const [key, plane] of attachment.planes)
      if (planes.get(key) !== plane) releasePlane(plane);
    attachment.planes = planes;
    arrangePlanes(attachment.below, below);
    arrangePlanes(attachment.above, above);
    rt.dirty = false;
  }
  function setSurfaces(
    session: Session,
    viewportRevision: bigint,
    layoutRevision: bigint,
    surfaces: MediaSurface[],
  ): void {
    const rt = session.runtime;
    refreshViewport(rt);
    if (!rt.viewport || !rt.attachment) throw failure("SurfaceUnavailable");
    if (viewportRevision !== rt.viewport.revision)
      throw domainFailure({
        tag: "StaleViewport",
        value: { currentRevision: rt.viewport.revision },
      });
    if (surfaces.length > capabilities.maxSurfacesPerSession)
      throw failure("InvalidSurface");
    const seen = new Set<number>();
    for (const surface of surfaces) {
      if (
        !Number.isInteger(surface.surfaceId) ||
        surface.surfaceId < 0 ||
        surface.surfaceId > 4294967295 ||
        seen.has(surface.surfaceId) ||
        !rectValid(surface.rect) ||
        !rectValid(surface.clip) ||
        !Number.isInteger(surface.cornerRadius) ||
        surface.cornerRadius < 0 ||
        surface.cornerRadius > 4294967295 ||
        !Number.isInteger(surface.depth) ||
        surface.depth < -2147483648 ||
        surface.depth > 2147483647
      )
        throw failure("InvalidSurface");
      seen.add(surface.surfaceId);
      // The core validates participant handles. It admits a participant before
      // the call handshake creates its peer here, so a picture may be placed
      // early; render() shows it once that peer's media is live.
    }
    const fingerprint = JSON.stringify(surfaces);
    if (
      session.layoutRevision !== undefined &&
      layoutRevision < session.layoutRevision
    )
      throw domainFailure({
        tag: "StaleLayout",
        value: { currentRevision: session.layoutRevision },
      });
    if (
      layoutRevision === session.layoutRevision &&
      session.layoutFingerprint !== undefined
    ) {
      if (fingerprint !== session.layoutFingerprint)
        throw failure("InvalidSurface");
      if (session.layoutViewportRevision === viewportRevision) return;
    }
    session.layoutRevision = layoutRevision;
    session.layoutFingerprint = fingerprint;
    session.layoutViewportRevision = viewportRevision;
    // Commands are trusted structured values, but retaining a caller-owned
    // mutable array would defeat validation and revision fencing.
    session.queued = surfaces.map((surface) => ({
      ...surface,
      rect: { ...surface.rect },
      clip: { ...surface.clip },
      source:
        surface.source.tag === "Local"
          ? { tag: "Local", value: { ...surface.source.value } }
          : { tag: "Remote", value: { ...surface.source.value } },
    }));
    rt.dirty = true;
    scheduleFrame();
  }

  async function command(
    rt: Runtime,
    command: MediaBackendCommand,
  ): Promise<MediaBackendResponse> {
    switch (command.tag) {
      case "RequestConsent":
        return requestConsent(
          rt,
          command.value.operationId,
          command.value.request,
        );
      case "OpenSession": {
        const { sessionId, operationId, tracks } = command.value;
        const op = operation(rt, operationId);
        op.status = "preparing";
        try {
          const session = createSession(rt, sessionId);
          op.session = session;
          op.opening = true;
          op.requested = { ...tracks };
          session.pending = op;
          updateIndicator(session);
          op.promise = prepare(op);
          return await op.promise;
        } catch (error) {
          if (op.status === "preparing") op.status = "failed";
          throw error;
        }
      }
      case "SetTracks": {
        const { sessionId, operationId, intentRevision, tracks } =
          command.value;
        const session = requireSession(rt, sessionId);
        if (!session.committed || intentRevision <= session.admittedRevision)
          throw failure("InvalidState");
        const op = operation(rt, operationId);
        op.status = "preparing";
        session.admittedRevision = intentRevision;
        if (session.pending) cancelOperation(session.pending);
        op.session = session;
        op.revision = intentRevision;
        op.requested = { ...tracks };
        session.pending = op;
        updateIndicator(session);
        op.promise = prepare(op);
        return op.promise;
      }
      case "CommitOperation": {
        const op = rt.operations.get(command.value.operationId);
        if (!op) throw failure("InvalidOperation");
        return commit(op);
      }
      case "CancelOperation": {
        const op =
          rt.operations.get(command.value.operationId) ??
          operation(rt, command.value.operationId);
        cancelOperation(op);
        return { tag: "Done" };
      }
      case "CloseSession": {
        const session = rt.sessions.get(command.value.sessionId);
        if (session) closeSession(session);
        return { tag: "Done" };
      }
      case "CreatePeer": {
        const value = command.value;
        await createPeer(
          requireSession(rt, value.sessionId),
          value.participantId,
          value.offerer,
        );
        return { tag: "Done" };
      }
      case "ApplyDescription": {
        const value = command.value;
        await applyDescription(
          requirePeer(requireSession(rt, value.sessionId), value.participantId),
          value.description,
        );
        return { tag: "Done" };
      }
      case "AddIceCandidate": {
        const value = command.value;
        await addCandidate(
          requirePeer(requireSession(rt, value.sessionId), value.participantId),
          value.candidate,
        );
        return { tag: "Done" };
      }
      case "RemovePeer": {
        const session = rt.sessions.get(command.value.sessionId);
        const peer = session?.peers.get(command.value.participantId);
        if (peer && !peer.closed) {
          publishPeer(peer, "Closed");
          closePeer(peer);
        }
        return { tag: "Done" };
      }
      case "SetSurfaces": {
        const value = command.value;
        setSurfaces(
          requireSession(rt, value.sessionId),
          value.viewportRevision,
          value.layoutRevision,
          value.surfaces,
        );
        return { tag: "Done" };
      }
      case "CloseRuntime":
        closeRuntime(rt);
        return { tag: "Done" };
    }
  }
  function revokePermission(
    rt: Runtime,
    permission: MediaRevokedPermission,
    source: MediaRevocationSource,
  ): void {
    if (rt.closed) return;
    const kind = permission === "Microphone" ? "microphone" : "camera";
    for (const op of rt.operations.values()) {
      if (
        permission !== "Calling" &&
        !op.requested?.[kind] &&
        !op.tracks[kind] &&
        !op.consents[permission]
      )
        continue;
      // Core has committed its intent before CommitOperation reaches us. Fence
      // its sender swap through teardown, never turn that commit into a cancel.
      if (op.status === "committing" && op.session) closeSession(op.session);
      else cancelOperation(op);
    }
    for (const session of rt.sessions.values()) {
      if (
        permission === "Calling" ||
        session.requested[kind] ||
        session.tracks[kind]
      )
        closeSession(session);
    }
    emit(rt, { tag: "PermissionRevoked", value: { permission, source } });
  }
  // Browser permission queries are observations, never prompts. Some Gecko
  // versions expose these only through native host permission observers; those
  // hosts must also call revokePermission when their OS grants are withdrawn.
  for (const name of ["microphone", "camera"] as const) {
    void win.navigator.permissions
      ?.query(webIdl<PermissionDescriptor>({ name: name as PermissionName }))
      .then((status) => {
        if (disposed) return;
        let previous = status.state;
        const changed = () => {
          if (
            !disposed &&
            previous === "granted" &&
            status.state !== "granted"
          ) {
            for (const rt of runtimes.values())
              revokePermission(
                rt,
                name === "microphone" ? "Microphone" : "Camera",
                "OperatingSystem",
              );
          }
          previous = status.state;
        };
        status.addEventListener("change", changed);
        permissionCleanups.push(() =>
          status.removeEventListener("change", changed),
        );
      })
      .catch(() => undefined);
  }
  const visibilityChanged = () => {
    for (const rt of runtimes.values()) if (!rt.closed) refreshViewport(rt);
    scheduleFrame();
  };
  const devicesChanged = () => {
    for (const rt of runtimes.values())
      for (const session of rt.sessions.values()) {
        if (alive(session)) {
          publishLocal(session);
          void updateAudioRoute(session);
        }
      }
  };
  doc.addEventListener("visibilitychange", visibilityChanged);
  win.navigator.mediaDevices?.addEventListener("devicechange", devicesChanged);
  return {
    async mediaBackendCapabilities(product) {
      authorize(product);
      return { ...capabilities, supported: supported() };
    },
    mediaBackendEvents(product, runtimeId) {
      authorize(product);
      return eventStream(runtime(runtimeId));
    },
    async mediaBackendCommand(product, runtimeId, request) {
      authorize(product);
      if (request.tag === "CloseRuntime") {
        const rt = runtimes.get(runtimeId) ?? runtime(runtimeId);
        closeRuntime(rt);
        return { tag: "Done" };
      }
      try {
        const cleanup =
          request.tag === "CloseSession" ||
          request.tag === "RemovePeer" ||
          request.tag === "CancelOperation";
        if (!cleanup && !supported()) throw failure("Unavailable");
        const productElement = options.getProductElement(runtimeId);
        if (
          !cleanup &&
          productElement &&
          !attachmentIsolated(productElement, runtimeId)
        ) {
          const existing = runtimes.get(runtimeId);
          if (existing) closeRuntime(existing, "HostFailure");
          throw failure();
        }
        return await command(runtime(runtimeId), request);
      } catch (error) {
        if (error instanceof CommandRejected)
          return { tag: "Rejected", value: { failure: error.failure } };
        throw failure();
      }
    },
    attach(runtimeId) {
      if (!supported()) throw failure("Unavailable");
      attach(runtime(runtimeId));
    },
    detach(runtimeId) {
      const rt = runtimes.get(runtimeId);
      if (rt && !rt.closed) detach(rt);
    },
    refreshViewport(runtimeId) {
      const rt = runtimes.get(runtimeId);
      if (rt && !rt.closed) refreshViewport(rt);
    },
    revokePermission(runtimeId, permission, source = "Product") {
      const rt = runtimes.get(runtimeId);
      if (rt) revokePermission(rt, permission, source);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      if (animationFrame !== undefined)
        win.cancelAnimationFrame(animationFrame);
      animationFrame = undefined;
      doc.removeEventListener("visibilitychange", visibilityChanged);
      win.navigator.mediaDevices?.removeEventListener(
        "devicechange",
        devicesChanged,
      );
      for (const remove of permissionCleanups.splice(0)) remove();
      for (const rt of runtimes.values()) closeRuntime(rt);
    },
  };
}
