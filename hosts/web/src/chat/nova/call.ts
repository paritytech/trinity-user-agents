import type { HexString } from "@parity/truapi/scale";
import {
  MEDIA_STATE,
  RENEGOTIATION,
  candidateLine,
  decodeCandidates,
  decodeChannelSignal,
  decodeFrame,
  decodeMediaState,
  decodeSetup,
  encodeCandidates,
  encodeChannelSignal,
  encodeFrame,
  encodeMediaState,
  encodeSetup,
  minimalCandidate,
  minimalSetup,
  setupSdp,
  type ChannelSignal,
  type MediaStateSignal,
  type MinimalCandidate,
} from "./call-wire.js";
import type { IncomingCallSignal, OutgoingCallSignal } from "./engine.js";

/**
 * One-to-one calls over the Polkadot app's call signaling (`./call-wire.ts`),
 * in the browser's WebRTC.
 *
 * A {@link CallSession} is one call, from the offer to its end. It opens the
 * camera and microphone only from {@link CallSession.start} or
 * {@link CallSession.accept}, which the screen calls on a user's press, and
 * stops every track it opened when the call ends, however it ends. A
 * {@link CallManager} holds at most one call for a chat provider: it routes
 * the peer's signals to it, answers a second offer as busy, and ends the call
 * when the provider stops.
 */

export type CallState =
  | "calling"
  | "ringing"
  | "incoming"
  | "connecting"
  | "connected"
  | "ended";

/** How long an outgoing call rings before it is canceled. */
export const RING_TIMEOUT_MS = 60_000;
/** An offer older than this is not rung, as the iOS app does. */
export const STALE_OFFER_MS = 30_000;
/** First candidates wait for at most this many, or this long. */
const INITIAL_CANDIDATES = 4;
const INITIAL_WAIT_MS = 500;
/** Candidates per kind 10 message, as the iOS app batches them. */
const CANDIDATE_BATCH = 12;

export interface CallDeps {
  /** Send a signal to the peer. Resolves to its message id. */
  send(signal: OutgoingCallSignal): Promise<string>;
  /** Open the microphone, and the camera for video. */
  media(video: boolean): Promise<MediaStream>;
  rtc: RTCConfiguration;
  /**
   * Relay servers to add for this call, such as TURN credentials. Asked once
   * per call, after the media opened; an empty list leaves `rtc` as it is.
   */
  relays?: () => Promise<RTCIceServer[]>;
  peerConnection?: (config: RTCConfiguration) => RTCPeerConnection;
  onChange(): void;
  ringTimeoutMs?: number;
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

function mediaProblem(error: unknown): string {
  const name = error instanceof DOMException ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError")
    return "Camera or microphone access was denied.";
  if (name === "NotFoundError") return "No camera or microphone was found.";
  return `The camera or microphone could not be opened: ${errorText(error)}`;
}

export class CallSession {
  state: CallState;
  /** Why the call ended or cannot go on, for the screen. */
  problem: string | null = null;
  /** The call's id: the offer's message id. Null until an outgoing offer is sent. */
  offerId: string | null;
  local: MediaStream | null = null;
  /** What the peer sends, once the connection exists. */
  remote: MediaStream | null = null;
  muted = false;
  cameraOn: boolean;
  remoteCamera: boolean;
  private peer: RTCPeerConnection | null = null;
  private channel: RTCDataChannel | null = null;
  private remoteSet = false;
  private readonly heldRemote: RTCIceCandidateInit[] = [];
  /** Local candidates before the first setup goes out, then those waiting for a kind 10 message. */
  private gathered: MinimalCandidate[] = [];
  private setupSent = false;
  private batchTimer: ReturnType<typeof setTimeout> | null = null;
  private ringTimer: ReturnType<typeof setTimeout> | null = null;

  private constructor(
    private readonly deps: CallDeps,
    readonly direction: "out" | "in",
    readonly video: boolean,
    offerId: string | null,
    private readonly offer: Uint8Array | null,
  ) {
    this.state = direction === "out" ? "calling" : "incoming";
    this.offerId = offerId;
    this.cameraOn = video;
    this.remoteCamera = video;
  }

  static outgoing(deps: CallDeps, video: boolean): CallSession {
    return new CallSession(deps, "out", video, null, null);
  }

  static incoming(
    deps: CallDeps,
    offerId: string,
    offer: Uint8Array,
    video: boolean,
  ): CallSession {
    return new CallSession(deps, "in", video, offerId, offer);
  }

  get ended(): boolean {
    return this.state === "ended";
  }

  private changed(): void {
    this.deps.onChange();
  }

  /** Open the media, or end the call saying why not. */
  private async openMedia(): Promise<boolean> {
    try {
      const stream = await this.deps.media(this.video);
      if (this.ended) {
        for (const track of stream.getTracks()) track.stop();
        return false;
      }
      this.local = stream;
      return true;
    } catch (error) {
      this.problem = mediaProblem(error);
      return false;
    }
  }

  /** The call's configuration: the configured servers, then any relays. */
  private async configuration(): Promise<RTCConfiguration> {
    const relays = (await this.deps.relays?.()) ?? [];
    return relays.length === 0
      ? this.deps.rtc
      : {
          ...this.deps.rtc,
          iceServers: [...(this.deps.rtc.iceServers ?? []), ...relays],
        };
  }

  private connect(config: RTCConfiguration): RTCPeerConnection {
    const create =
      this.deps.peerConnection ??
      ((config: RTCConfiguration) => new RTCPeerConnection(config));
    const peer = create(config);
    this.peer = peer;
    const remote = new MediaStream();
    this.remote = remote;
    peer.addEventListener("icecandidate", (event) => {
      if (event.candidate === null || this.ended) return;
      this.localCandidate(event.candidate);
    });
    peer.addEventListener("track", (event) => {
      remote.addTrack(event.track);
      this.changed();
    });
    peer.addEventListener("connectionstatechange", () => {
      if (this.ended) return;
      if (peer.connectionState === "connected" && this.state !== "connected") {
        this.state = "connected";
        this.clearRing();
        this.changed();
      }
      if (peer.connectionState === "failed") {
        this.problem = "The connection failed.";
        this.hangUp();
      }
    });
    peer.addEventListener("datachannel", (event) =>
      this.useChannel(event.channel),
    );
    return peer;
  }

  /**
   * Run one step of the call. A failure ends the call, tells the peer, and
   * says why; a step that fails because the call already ended says nothing.
   */
  private async step(work: () => Promise<void>): Promise<void> {
    try {
      await work();
    } catch (error) {
      if (this.ended) return;
      this.problem = `The call failed: ${errorText(error)}`;
      this.hangUp();
    }
  }

  /** Call the peer: open the media, then send the first offer. */
  start(): Promise<void> {
    return this.step(() => this.offerCall());
  }

  private async offerCall(): Promise<void> {
    if (!(await this.openMedia())) {
      this.end();
      return;
    }
    const config = await this.configuration();
    if (this.ended) return;
    const peer = this.connect(config);
    this.useChannel(peer.createDataChannel("dataChannel"));
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    const candidates = await this.initialCandidates();
    if (this.ended) return;
    const setup = minimalSetup(offer.sdp ?? "", candidates);
    try {
      this.offerId = await this.deps.send({
        kind: "call-offer",
        sdp: encodeSetup(setup),
        video: this.video,
      });
    } catch (error) {
      this.problem = `The call was not sent: ${errorText(error)}`;
      this.end();
      return;
    }
    this.setupSent = true;
    this.ringTimer = setTimeout(() => {
      if (this.state === "calling" || this.state === "ringing") {
        this.problem = "No answer.";
        this.hangUp();
      }
    }, this.deps.ringTimeoutMs ?? RING_TIMEOUT_MS);
    this.changed();
  }

  /** The peer's device has the offer: it rings there. */
  delivered(): void {
    if (this.state !== "calling") return;
    this.state = "ringing";
    this.changed();
  }

  /** Answer an incoming call: open the media, then send the first answer. */
  accept(): Promise<void> {
    return this.step(() => this.answerCall());
  }

  private async answerCall(): Promise<void> {
    if (this.state !== "incoming" || this.offer === null) return;
    this.problem = null;
    this.state = "connecting";
    this.changed();
    if (!(await this.openMedia())) {
      if (!this.ended) {
        this.state = "incoming";
        this.changed();
      }
      return;
    }
    const config = await this.configuration();
    if (this.ended) return;
    const peer = this.connect(config);
    const offer = decodeSetup(this.offer);
    await peer.setRemoteDescription({ type: "offer", sdp: setupSdp(offer) });
    await this.remoteReady(
      offer.candidates.map((candidate) => ({
        candidate: candidateLine(candidate),
        sdpMid: "0",
        sdpMLineIndex: 0,
      })),
    );
    const answer = await peer.createAnswer();
    await peer.setLocalDescription(answer);
    const candidates = await this.initialCandidates();
    if (this.ended) return;
    try {
      await this.deps.send({
        kind: "call-answer",
        offerId: this.offerId!,
        sdp: encodeSetup(minimalSetup(answer.sdp ?? "", candidates)),
      });
    } catch (error) {
      this.problem = `The answer was not sent: ${errorText(error)}`;
      this.end();
      return;
    }
    this.setupSent = true;
    this.flushCandidates();
  }

  /** A signal from the peer for this call. */
  receive(content: IncomingCallSignal["content"]): Promise<void> {
    return this.step(() => this.follow(content));
  }

  private async follow(content: IncomingCallSignal["content"]): Promise<void> {
    if (this.ended) return;
    switch (content.kind) {
      case "call-offer":
        return;
      case "call-closed":
        this.problem ??=
          this.state === "incoming" ? "Missed call." : "The call ended.";
        this.end();
        return;
      case "call-answer": {
        if (this.direction !== "out" || this.peer === null || this.remoteSet)
          return;
        this.clearRing();
        this.state = "connecting";
        this.changed();
        const answer = decodeSetup(content.sdp);
        await this.peer.setRemoteDescription({
          type: "answer",
          sdp: setupSdp(answer),
        });
        await this.remoteReady(
          answer.candidates.map((candidate) => ({
            candidate: candidateLine(candidate),
            sdpMid: "0",
            sdpMLineIndex: 0,
          })),
        );
        this.flushCandidates();
        return;
      }
      case "call-candidates":
        await this.addRemote(
          decodeCandidates(content.sdp).map((candidate) => ({
            candidate: candidateLine(candidate),
            sdpMid: "0",
            sdpMLineIndex: 0,
          })),
        );
    }
  }

  private async remoteReady(candidates: RTCIceCandidateInit[]): Promise<void> {
    this.remoteSet = true;
    await this.addRemote([...this.heldRemote.splice(0), ...candidates]);
  }

  private async addRemote(candidates: RTCIceCandidateInit[]): Promise<void> {
    if (!this.remoteSet || this.peer === null) {
      this.heldRemote.push(...candidates);
      return;
    }
    for (const candidate of candidates)
      await this.peer.addIceCandidate(candidate).catch(() => {});
  }

  /** The first candidates, for the setup: a few, or what came in a short wait. */
  private initialCandidates(): Promise<MinimalCandidate[]> {
    return new Promise((resolve) => {
      const done = () => {
        clearInterval(poll);
        clearTimeout(timer);
        resolve(this.gathered.splice(0, INITIAL_CANDIDATES));
      };
      const poll = setInterval(() => {
        if (this.gathered.length >= INITIAL_CANDIDATES || this.ended) done();
      }, 20);
      const timer = setTimeout(done, INITIAL_WAIT_MS);
    });
  }

  private localCandidate(candidate: RTCIceCandidate): void {
    if (this.channel?.readyState === "open") {
      this.sendSignal({
        kind: "candidates",
        candidates: [
          {
            candidate: candidate.candidate,
            sdpMLineIndex: candidate.sdpMLineIndex ?? 0,
            sdpMid: candidate.sdpMid,
          },
        ],
      });
      return;
    }
    const compact = minimalCandidate(candidate.candidate);
    if (compact === null || compact.transport !== "udp") return;
    this.gathered.push(compact);
    if (this.setupSent && this.remoteSet) this.scheduleCandidates();
  }

  private scheduleCandidates(): void {
    if (this.gathered.length >= CANDIDATE_BATCH) this.flushCandidates();
    else
      this.batchTimer ??= setTimeout(
        () => this.flushCandidates(),
        INITIAL_WAIT_MS,
      );
  }

  /** Send the candidates gathered since the setup as one kind 10 message. */
  private flushCandidates(): void {
    if (this.batchTimer !== null) clearTimeout(this.batchTimer);
    this.batchTimer = null;
    if (this.ended || this.offerId === null) return;
    const batch = this.gathered.splice(0, CANDIDATE_BATCH);
    if (batch.length === 0) return;
    void this.deps
      .send({
        kind: "call-candidates",
        offerId: this.offerId,
        sdp: encodeCandidates(batch),
      })
      .catch(() => {});
    if (this.gathered.length > 0) this.scheduleCandidates();
  }

  private useChannel(channel: RTCDataChannel): void {
    this.channel = channel;
    channel.binaryType = "arraybuffer";
    channel.addEventListener("open", () => {
      if (this.ended) return;
      this.sendMediaState({ camera: this.cameraOn });
      if (this.direction === "out") void this.step(() => this.offerMedia());
    });
    channel.addEventListener("message", (event) => {
      if (!(event.data instanceof ArrayBuffer)) return;
      const data = new Uint8Array(event.data);
      void this.step(() => this.onChannel(data));
    });
  }

  private sendFrame(id: string, data: Uint8Array): void {
    if (this.channel?.readyState !== "open") return;
    this.channel.send(encodeFrame(id, data) as Uint8Array<ArrayBuffer>);
  }

  private sendSignal(signal: ChannelSignal): void {
    this.sendFrame(RENEGOTIATION, encodeChannelSignal(signal));
  }

  private sendMediaState(state: MediaStateSignal): void {
    this.sendFrame(MEDIA_STATE, encodeMediaState(state));
  }

  /** The caller adds its tracks and negotiates media over the channel. */
  private async offerMedia(): Promise<void> {
    const peer = this.peer;
    if (peer === null || this.local === null) return;
    for (const track of this.local.getTracks())
      peer.addTrack(track, this.local);
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    this.sendSignal({ kind: "offer", sdp: offer.sdp ?? "" });
  }

  private async onChannel(bytes: Uint8Array): Promise<void> {
    const peer = this.peer;
    if (peer === null || this.ended) return;
    let frame: { id: string; data: Uint8Array };
    try {
      frame = decodeFrame(bytes);
    } catch {
      return;
    }
    if (frame.id === MEDIA_STATE) {
      const state = decodeMediaState(frame.data);
      if (state !== null && "camera" in state) this.remoteCamera = state.camera;
      this.changed();
      return;
    }
    if (frame.id !== RENEGOTIATION) return;
    const signal = decodeChannelSignal(frame.data);
    if (signal === null) return;
    if (signal.kind === "offer" && this.direction === "in") {
      await peer.setRemoteDescription({ type: "offer", sdp: signal.sdp });
      if (this.local !== null)
        for (const track of this.local.getTracks())
          peer.addTrack(track, this.local);
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      this.sendSignal({ kind: "answer", sdp: answer.sdp ?? "" });
    } else if (signal.kind === "answer" && this.direction === "out")
      await peer.setRemoteDescription({ type: "answer", sdp: signal.sdp });
    else if (signal.kind === "candidates")
      for (const entry of signal.candidates)
        await peer
          .addIceCandidate({
            candidate: entry.candidate,
            sdpMid: entry.sdpMid,
            sdpMLineIndex: entry.sdpMLineIndex,
          })
          .catch(() => {});
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    for (const track of this.local?.getAudioTracks() ?? [])
      track.enabled = !muted;
    this.sendMediaState({ microphone: !muted });
    this.changed();
  }

  setCamera(on: boolean): void {
    this.cameraOn = on;
    for (const track of this.local?.getVideoTracks() ?? []) track.enabled = on;
    this.sendMediaState({ camera: on });
    this.changed();
  }

  /** End the call and tell the peer: hang up, cancel, or decline. */
  hangUp(): void {
    if (this.ended) return;
    const offerId = this.offerId;
    this.end();
    if (offerId !== null)
      void this.deps.send({ kind: "call-closed", offerId }).catch(() => {});
  }

  /** End the call here. Every track this call opened stops. */
  end(): void {
    if (this.ended) return;
    this.state = "ended";
    this.clearRing();
    if (this.batchTimer !== null) clearTimeout(this.batchTimer);
    this.batchTimer = null;
    this.channel?.close();
    this.peer?.close();
    for (const track of this.local?.getTracks() ?? []) track.stop();
    for (const track of this.remote?.getTracks() ?? [])
      this.remote?.removeTrack(track);
    this.changed();
  }

  private clearRing(): void {
    if (this.ringTimer !== null) clearTimeout(this.ringTimer);
    this.ringTimer = null;
  }
}

/** What a manager needs from the chat it calls through. */
export interface CallManagerDeps {
  send(account: HexString, signal: OutgoingCallSignal): Promise<string>;
  media(video: boolean): Promise<MediaStream>;
  rtc: RTCConfiguration;
  /** Relay servers for a call; see {@link CallDeps.relays}. */
  relays?: () => Promise<RTCIceServer[]>;
  now(): number;
  peerConnection?: (config: RTCConfiguration) => RTCPeerConnection;
  ringTimeoutMs?: number;
}

/** The call a manager holds, and whom it is with. */
export interface HeldCall {
  account: HexString;
  session: CallSession;
}

export class CallManager {
  private held: HeldCall | null = null;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: CallManagerDeps) {}

  current(): HeldCall | null {
    return this.held;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of [...this.listeners]) listener();
  }

  private session(account: HexString): CallDeps {
    return {
      send: (signal) => this.deps.send(account, signal),
      media: this.deps.media,
      rtc: this.deps.rtc,
      relays: this.deps.relays,
      peerConnection: this.deps.peerConnection,
      ringTimeoutMs: this.deps.ringTimeoutMs,
      onChange: () => this.changed(),
    };
  }

  private busy(): boolean {
    return this.held !== null && !this.held.session.ended;
  }

  /** Call `account`. Refused while another call is on. */
  async start(account: HexString, video: boolean): Promise<void> {
    if (this.busy()) throw new Error("A call is already on.");
    const session = CallSession.outgoing(this.session(account), video);
    this.held = { account, session };
    this.changed();
    await session.start();
  }

  /** A call signal from a peer. A second offer while a call is on is answered as busy. */
  receive(signal: IncomingCallSignal): void {
    const content = signal.content;
    if (content.kind === "call-offer") {
      const stale = this.deps.now() - signal.at > STALE_OFFER_MS;
      if (stale || this.busy()) {
        void this.deps
          .send(signal.account, { kind: "call-closed", offerId: signal.id })
          .catch(() => {});
        return;
      }
      this.held = {
        account: signal.account,
        session: CallSession.incoming(
          this.session(signal.account),
          signal.id,
          content.sdp,
          content.video,
        ),
      };
      this.changed();
      return;
    }
    const held = this.held;
    if (
      held === null ||
      held.account !== signal.account ||
      held.session.offerId !== content.offerId
    )
      return;
    void held.session.receive(content);
  }

  /** Forget a call that ended, once the screen has shown it. */
  clear(): void {
    if (this.held?.session.ended) {
      this.held = null;
      this.changed();
    }
  }

  /** End the call, telling the peer, and stop following. */
  dispose(): void {
    this.held?.session.hangUp();
    this.held = null;
    this.listeners.clear();
  }
}
