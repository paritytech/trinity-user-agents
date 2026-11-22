import { afterEach, describe, expect, it, mock } from "bun:test";
import { asProductId } from "@parity/truapi";
import { Window } from "happy-dom";

import type {
  MediaBackendCommand,
  MediaBackendEvent,
  ProductContext,
} from "../generated/host-callbacks.js";
import { createBrowserMediaBackend } from "./browser-media-backend.js";

const product: ProductContext = {
  productId: asProductId("media-test.dot"),
  executionKind: "App",
};
const sessionId = `0x${"01".repeat(32)}` as const;
const participantId = `0x${"02".repeat(32)}` as const;
const operationId = `0x${"03".repeat(32)}` as const;
const embedded = "candidate:1 1 UDP 100 192.0.2.1 5000 typ host generation 0";
const late = "candidate:2 1 UDP 90 192.0.2.2 5001 typ relay generation 0";

function sdp(ufrag = "initial", candidates: readonly string[] = []): string {
  return [
    "v=0",
    `a=ice-ufrag:${ufrag}`,
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:audio",
    ...candidates.map((candidate) => `a=${candidate}`),
    "m=video 9 UDP/TLS/RTP/SAVPF 96",
    "a=mid:camera",
    "m=video 9 UDP/TLS/RTP/SAVPF 96",
    "a=mid:screen",
    "",
  ].join("\r\n");
}

function candidate(
  value: string,
  options: Partial<RTCIceCandidateInit> = {},
): RTCIceCandidate {
  return {
    candidate: value,
    sdpMid: "audio",
    sdpMLineIndex: 0,
    usernameFragment: "initial",
    ...options,
  } as RTCIceCandidate;
}

class TestTrack {
  constructor(readonly kind: string) {}
  clone() {
    return new TestTrack(this.kind);
  }
  getSettings() {
    return {};
  }
  stop() {}
}

class TestSender {
  replaceTrack = mock(async (_track: unknown) => {});
}

class TestPeerConnection extends EventTarget {
  signalingState = "stable";
  connectionState = "new";
  iceGatheringState = "new";
  localDescription: RTCSessionDescriptionInit | null = null;
  remoteDescription: RTCSessionDescriptionInit | null = null;
  ufrag = "initial";
  onicecandidate: ((event: { candidate: RTCIceCandidate | null }) => void) | null = null;
  onnegotiationneeded: (() => void) | null = null;
  onconnectionstatechange: (() => void) | null = null;
  ontrack: unknown = null;
  slots: {
    mid: string;
    direction: RTCRtpTransceiverDirection;
    sender: TestSender;
    receiver: { track: TestTrack };
  }[] = [];
  gatheringListeners = new Set<EventListenerOrEventListenerObject>();
  afterSetLocal?: (pc: TestPeerConnection) => void | Promise<void>;

  override addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
  ) {
    if (type === "icegatheringstatechange" && listener)
      this.gatheringListeners.add(listener);
    super.addEventListener(type, listener);
  }
  override removeEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject | null,
  ) {
    if (type === "icegatheringstatechange" && listener)
      this.gatheringListeners.delete(listener);
    super.removeEventListener(type, listener);
  }
  addTransceiver(kind: string, init: RTCRtpTransceiverInit) {
    const slot = {
      mid: ["audio", "camera", "screen"][this.slots.length],
      direction: init.direction ?? "sendrecv",
      sender: new TestSender(),
      receiver: { track: new TestTrack(kind) },
    };
    this.slots.push(slot);
    return slot;
  }
  getTransceivers() {
    return this.slots;
  }
  async createOffer(): Promise<RTCSessionDescriptionInit> {
    return { type: "offer", sdp: sdp(this.ufrag) };
  }
  async createAnswer(): Promise<RTCSessionDescriptionInit> {
    return { type: "answer", sdp: sdp(this.ufrag) };
  }
  async setLocalDescription(description: RTCSessionDescriptionInit) {
    this.localDescription = { ...description };
    this.signalingState = description.type === "offer" ? "have-local-offer" : "stable";
    this.iceGatheringState = "gathering";
    await this.afterSetLocal?.(this);
  }
  async setRemoteDescription(description: RTCSessionDescriptionInit) {
    this.remoteDescription = { ...description };
    this.signalingState = description.type === "offer" ? "have-remote-offer" : "stable";
    if (!this.slots.length) {
      for (const kind of ["audio", "video", "video"])
        this.addTransceiver(kind, { direction: "recvonly" });
    }
  }
  async addIceCandidate(_candidate: RTCIceCandidateInit) {}
  emitCandidate(value: RTCIceCandidate | null) {
    this.onicecandidate?.({ candidate: value });
  }
  completeGathering() {
    this.iceGatheringState = "complete";
    this.dispatchEvent(new Event("icegatheringstatechange"));
    this.emitCandidate(null);
  }
  restartIce() {
    this.ufrag = "restart";
    this.onnegotiationneeded?.();
  }
  close() {
    this.connectionState = "closed";
    this.signalingState = "closed";
  }
}

async function flushMicrotasks(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setImmediate(resolve);
  await promise;
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
});

async function fixture(afterSetLocal?: TestPeerConnection["afterSetLocal"]) {
  const window = new Window({ url: "https://host.test" });
  const timers = new Map<number, { callback: () => void; delay: number }>();
  const peers: TestPeerConnection[] = [];
  let nextTimer = 0;
  Object.defineProperties(window, {
    isSecureContext: { value: true, configurable: true },
    RTCPeerConnection: {
      value: class extends TestPeerConnection {
        constructor() {
          super();
          this.afterSetLocal = afterSetLocal;
          peers.push(this);
        }
      },
    },
    RTCRtpSender: { value: TestSender },
    MediaStreamTrack: { value: TestTrack },
    MediaStream: { value: class { addTrack(_track: unknown) {} } },
    DOMMatrixReadOnly: { value: class {} },
    requestAnimationFrame: { value: () => 1 },
    cancelAnimationFrame: { value: () => {} },
    setTimeout: {
      value: (callback: () => void, delay: number) => {
        const id = ++nextTimer;
        timers.set(id, { callback, delay });
        return id;
      },
    },
    clearTimeout: { value: (id: number) => timers.delete(id) },
  });
  Object.defineProperties(window.navigator, {
    mediaDevices: {
      value: Object.assign(new EventTarget(), {
        getUserMedia: mock(async () => { throw new Error("Unexpected capture"); }),
        getSupportedConstraints: () => ({ echoCancellation: true }),
      }),
    },
    permissions: { value: undefined },
  });
  Object.defineProperty(window.CSS, "supports", { value: () => true });
  Object.defineProperty(window.HTMLMediaElement.prototype, "srcObject", {
    configurable: true,
    writable: true,
    value: null,
  });
  const backend = createBrowserMediaBackend({
    window: window as unknown as globalThis.Window,
    document: window.document as unknown as Document,
    productId: product.productId,
    indicatorMount: window.document.body as unknown as Element,
    getProductElement: () => null,
    getCompositorMount: () => null,
    isProductIsolated: () => true,
    requestConsent: async () => { throw new Error("Unexpected consent"); },
  });
  const events: MediaBackendEvent[] = [];
  const observer = (async () => {
    for await (const item of backend.mediaBackendEvents(product, 1n)) {
      if (item.isErr()) throw new Error(item.error.reason);
      events.push(item.value);
    }
  })();
  cleanups.push(async () => {
    backend.dispose();
    await observer;
    await window.happyDOM.close();
  });
  const command = (request: MediaBackendCommand) =>
    backend.mediaBackendCommand(product, 1n, request);
  expect(await backend.mediaBackendCapabilities(product)).toMatchObject({
    supported: true,
  });
  expect(await command({
    tag: "OpenSession",
    value: {
      sessionId,
      operationId,
      tracks: { microphone: false, camera: false, screen: false },
    },
  })).toEqual({ tag: "Done" });
  await command({ tag: "CommitOperation", value: { operationId } });
  return {
    backend, command, peers, events, timers,
    signals: () =>
      events.filter(
        (event) => event.tag === "Description" || event.tag === "IceCandidate",
      ),
    async createPeer(offerer = true) {
      expect(
        await command({
          tag: "CreatePeer",
          value: { sessionId, participantId, offerer },
        }),
      ).toEqual({ tag: "Done" });
      await flushMicrotasks();
      return peers[0];
    },
    apply(kind: "Offer" | "Answer") {
      return command({
        tag: "ApplyDescription",
        value: { sessionId, participantId, description: { kind, sdp: sdp("remote") } },
      });
    },
    expireGathering() {
      const gathering = [...timers.entries()].filter(
        ([, timer]) => timer.delay === 1_000,
      );
      expect(gathering).toHaveLength(1);
      const [id, timer] = gathering[0];
      timers.delete(id);
      timer.callback();
    },
    gatheringTimers: () => [...timers.values()].filter((timer) => timer.delay === 1_000),
  };
}

function descriptionEvent(kind: "Offer" | "Answer", value: string) {
  return {
    tag: "Description",
    value: { sessionId, participantId, description: { kind, sdp: value } },
  };
}
function candidateEvent(value: string, mid = "audio", mlineIndex = 0) {
  return {
    tag: "IceCandidate",
    value: {
      sessionId,
      participantId,
      candidate: { candidate: value, mid, mlineIndex },
    },
  };
}

describe("browser Media half-trickle", () => {
  it("embeds candidates gathered before setLocalDescription resolves and suppresses delayed duplicate events", async () => {
    const f = await fixture((pc) => {
      pc.localDescription!.sdp = sdp("initial", [embedded]);
      pc.emitCandidate(candidate(`${embedded} ufrag initial`));
    });
    const pc = await f.createPeer();
    expect(f.signals()).toEqual([]);
    expect(f.gatheringTimers()).toHaveLength(1);
    pc.completeGathering();
    await flushMicrotasks();
    pc.emitCandidate(candidate(embedded, { sdpMid: null }));
    pc.emitCandidate(candidate(`${embedded} ufrag initial`));
    pc.emitCandidate(candidate(""));
    await flushMicrotasks();
    expect(f.signals()).toEqual([descriptionEvent("Offer", sdp("initial", [embedded]))]);
    expect(f.gatheringTimers()).toEqual([]);
    expect(pc.gatheringListeners.size).toBe(0);
  });

  it("publishes the current SDP at the deadline, then trickles only candidates absent from that snapshot", async () => {
    const f = await fixture();
    const pc = await f.createPeer();
    pc.localDescription!.sdp = sdp("initial", [embedded]);
    pc.emitCandidate(candidate(embedded));
    expect(f.signals()).toEqual([]);
    f.expireGathering();
    await flushMicrotasks();
    pc.localDescription!.sdp = sdp("initial", [embedded, late]);
    pc.emitCandidate(candidate(late));
    pc.emitCandidate(candidate(embedded));
    // The same endpoint on a different m-line was not embedded there.
    pc.emitCandidate(candidate(embedded, { sdpMid: "camera", sdpMLineIndex: 1 }));
    pc.completeGathering();
    await flushMicrotasks();
    expect(f.signals()).toEqual([
      descriptionEvent("Offer", sdp("initial", [embedded])),
      candidateEvent(late),
      candidateEvent(embedded, "camera", 1),
    ]);
    expect(pc.gatheringListeners.size).toBe(0);
    expect(f.gatheringTimers()).toEqual([]);
  });

  it("does not arm a deadline when gathering completed during setLocalDescription", async () => {
    const f = await fixture((pc) => {
      pc.localDescription!.sdp = sdp("initial", [embedded]);
      pc.completeGathering();
    });
    const pc = await f.createPeer();
    expect(f.signals()).toEqual([descriptionEvent("Offer", sdp("initial", [embedded]))]);
    expect(f.gatheringTimers()).toEqual([]);
    expect(pc.gatheringListeners.size).toBe(0);
  });

  it("waits for answer gathering and publishes the answer before any remaining candidate", async () => {
    const f = await fixture();
    const pc = await f.createPeer(false);
    const answer = f.apply("Offer");
    await flushMicrotasks();
    expect(f.signals()).toEqual([]);
    pc.localDescription!.sdp = sdp("initial", [embedded]);
    pc.emitCandidate(candidate(embedded));
    pc.emitCandidate(candidate(late));
    pc.completeGathering();
    expect(await answer).toEqual({ tag: "Done" });
    await flushMicrotasks();
    expect(f.signals()).toEqual([
      descriptionEvent("Answer", sdp("initial", [embedded])),
      candidateEvent(late),
    ]);
  });

  it("starts a new gathering window for an ICE restart and drops stale-generation candidates", async () => {
    const f = await fixture();
    const pc = await f.createPeer();
    pc.localDescription!.sdp = sdp("initial", [embedded]);
    pc.completeGathering();
    await flushMicrotasks();
    await f.apply("Answer");
    pc.connectionState = "disconnected";
    pc.onconnectionstatechange?.();
    await flushMicrotasks();
    expect(f.gatheringTimers()).toHaveLength(1);
    expect(f.signals()).toHaveLength(1);
    pc.localDescription!.sdp = sdp("restart", [embedded]);
    pc.emitCandidate(candidate(embedded, { usernameFragment: "restart" }));
    pc.completeGathering();
    await flushMicrotasks();
    pc.emitCandidate(candidate(late));
    pc.emitCandidate(candidate(late, { usernameFragment: "restart" }));
    await flushMicrotasks();
    expect(f.signals()).toEqual([
      descriptionEvent("Offer", sdp("initial", [embedded])),
      descriptionEvent("Offer", sdp("restart", [embedded])),
      candidateEvent(late),
    ]);
  });

  it("does not publish or start gathering after closure during setLocalDescription", async () => {
    const gate = Promise.withResolvers<void>();
    const f = await fixture(async (pc) => {
      pc.localDescription!.sdp = sdp("initial", [embedded]);
      pc.emitCandidate(candidate(embedded));
      await gate.promise;
    });
    const pc = await f.createPeer(false);
    const answer = f.apply("Offer");
    await flushMicrotasks();
    expect(f.gatheringTimers()).toEqual([]);
    await f.command({ tag: "RemovePeer", value: { sessionId, participantId } });
    gate.resolve();
    expect(await answer).toEqual({ tag: "Done" });
    await flushMicrotasks();
    expect(f.signals()).toEqual([]);
    expect(f.gatheringTimers()).toEqual([]);
    expect(pc.gatheringListeners.size).toBe(0);
  });

  it("discards buffered candidates when setting the local offer fails", async () => {
    const f = await fixture((pc) => {
      pc.emitCandidate(candidate(embedded));
      throw new Error("Local description rejected");
    });
    const pc = await f.createPeer();
    expect(f.signals()).toEqual([]);
    expect(f.events).toContainEqual({
      tag: "PeerStateChanged",
      value: { sessionId, participantId, state: "Failed" },
    });
    expect(pc.connectionState).toBe("closed");
    expect(pc.gatheringListeners.size).toBe(0);
    expect(f.gatheringTimers()).toEqual([]);
  });

  for (const close of [
    "RemovePeer",
    "CloseSession",
    "CloseRuntime",
    "dispose",
  ] as const) {
    it(`cancels answer gathering on ${close} without late signaling`, async () => {
      const f = await fixture();
      const pc = await f.createPeer(false);
      const answer = f.apply("Offer");
      await flushMicrotasks();
      expect(f.gatheringTimers()).toHaveLength(1);
      const candidateCallback = pc.onicecandidate!;
      if (close === "dispose") f.backend.dispose();
      else if (close === "RemovePeer")
        await f.command({ tag: close, value: { sessionId, participantId } });
      else if (close === "CloseSession")
        await f.command({ tag: close, value: { sessionId } });
      else await f.command({ tag: close });
      expect(await answer).toEqual({ tag: "Done" });
      pc.completeGathering();
      candidateCallback({ candidate: candidate(late) });
      await flushMicrotasks();
      expect(f.signals()).toEqual([]);
      expect(f.gatheringTimers()).toEqual([]);
      expect(pc.gatheringListeners.size).toBe(0);
      expect(pc.onicecandidate).toBeNull();
      expect(pc.connectionState).toBe("closed");
    });
  }
});
