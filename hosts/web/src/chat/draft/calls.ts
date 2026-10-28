import type { CallSupport, CallView } from "../provider.js";
import type { ChatRoomCall } from "./contract.js";
import type { ChatV2Host } from "./host.js";

const RING_MS = 60_000;

/**
 * The calls of one product's v2 rooms, through the existing `CallSupport`.
 * The product owns the call: it rings, places and ends it through
 * `setRoomCall` and its own signaling. The host owns the devices and the
 * media, which run only through `callMedia.start` after the user pressed a
 * button here. A room that declares no calls has its buttons disabled with
 * the reason, as `CallSupport.blocked` draws them.
 */
export class V2Calls implements CallSupport {
  private readonly listeners = new Set<() => void>();
  private outgoing: {
    roomId: string;
    video: boolean;
    timer: ReturnType<typeof setTimeout>;
  } | null = null;
  private ended: CallView | null = null;
  private last: CallView | null = null;
  /** The call the user placed, once the product showed it. */
  private placed: string | null = null;
  private muted = false;
  private cameraOn = true;

  constructor(
    private readonly productId: string,
    private readonly host: ChatV2Host,
    private readonly title: (roomId: string) => string,
  ) {}

  /** The call buttons the room declared, or null for none. */
  private declared(roomId: string): { audio: boolean; video: boolean } | null {
    const calls = this.host.room(this.productId, roomId)?.features?.calls;
    return calls === undefined || (!calls.audio && !calls.video) ? null : calls;
  }

  blocked(roomId: string): string | null {
    if (!this.host.connected(this.productId))
      return "The product's worker is not running.";
    if (this.declared(roomId) === null) return "This room has no calls.";
    if (this.held() !== null || this.outgoing !== null)
      return "A call is already on.";
    return null;
  }

  async start(roomId: string, video: boolean): Promise<void> {
    const blocked = this.blocked(roomId);
    if (blocked !== null) throw new Error(blocked);
    if (!this.declared(roomId)?.[video ? "video" : "audio"])
      throw new Error(`This room has no ${video ? "video" : "audio"} calls.`);
    this.ended = null;
    this.muted = false;
    this.cameraOn = video;
    this.host.roomEvent(this.productId, roomId, {
      tag: "CallRequested",
      value: { video },
    });
    this.outgoing = {
      roomId,
      video,
      timer: setTimeout(() => {
        this.outgoing = null;
        this.end("No answer.");
      }, RING_MS),
    };
    this.changed();
  }

  /** The room that shows a call, with its call. */
  private held(): { roomId: string; call: ChatRoomCall } | null {
    for (const room of this.host.roomsOf(this.productId))
      if (room.call !== undefined)
        return { roomId: room.roomId, call: room.call };
    return null;
  }

  current(): CallView | null {
    const held = this.held();
    if (held !== null) {
      const { roomId, call } = held;
      const media = this.host.heldMedia(this.productId);
      const ours =
        media !== null &&
        media.roomId === roomId &&
        media.callId === call.value.callId;
      return {
        conversationId: roomId,
        title: `${
          call.tag === "Ringing" && call.value.caller !== undefined
            ? call.value.caller
            : this.title(roomId)
        }${ours && media.engine === "synthetic" ? " · synthetic call, no audio or video" : ""}`,
        video: call.value.video,
        direction:
          this.placed === call.value.callId || this.outgoing?.roomId === roomId
            ? "out"
            : "in",
        state:
          call.tag === "Ringing"
            ? "incoming"
            : ours
              ? media.session.state
              : "connected",
        problem: null,
        muted: this.muted,
        cameraOn: call.value.video && this.cameraOn,
        remoteCamera: call.value.video,
        local: ours ? media.session.local : null,
        remote: ours ? media.session.remote : null,
        relay: ours
          ? null
          : "The product runs this call. This host runs no media for it.",
      };
    }
    if (this.outgoing !== null)
      return {
        conversationId: this.outgoing.roomId,
        title: this.title(this.outgoing.roomId),
        video: this.outgoing.video,
        direction: "out",
        state: "calling",
        problem: null,
        muted: this.muted,
        cameraOn: this.outgoing.video && this.cameraOn,
        remoteCamera: false,
        local: null,
        remote: null,
        relay: null,
      };
    return this.ended;
  }

  async accept(): Promise<void> {
    const held = this.held();
    if (held?.call.tag !== "Ringing") return;
    this.ended = null;
    this.muted = false;
    this.cameraOn = held.call.value.video;
    this.host.roomEvent(this.productId, held.roomId, {
      tag: "CallAnswered",
      value: { callId: held.call.value.callId, accept: true },
    });
  }

  hangUp(): void {
    const held = this.held();
    if (held?.call.tag === "Ringing")
      this.host.roomEvent(this.productId, held.roomId, {
        tag: "CallAnswered",
        value: { callId: held.call.value.callId, accept: false },
      });
    else if (held?.call.tag === "Active") {
      this.host.stopMedia(this.productId);
      this.host.roomEvent(this.productId, held.roomId, {
        tag: "CallEnded",
        value: { callId: held.call.value.callId },
      });
    }
    this.clearOutgoing();
    this.end(null);
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    this.host.heldMedia(this.productId)?.session.setMuted(muted);
    this.changed();
  }

  setCamera(on: boolean): void {
    this.cameraOn = on;
    this.host.heldMedia(this.productId)?.session.setCamera(on);
    this.changed();
  }

  dismiss(): void {
    this.ended = null;
    this.changed();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The room's call changed: follow it, and show an ended call once it goes. */
  changed(): void {
    const held = this.held();
    if (held !== null && this.outgoing?.roomId === held.roomId)
      this.placed = held.call.value.callId;
    if (held !== null) this.clearOutgoing();
    const now = this.current();
    if (now === null && this.last !== null && this.last.state !== "ended")
      this.ended = { ...this.last, state: "ended", local: null, remote: null };
    this.last = this.current();
    for (const listener of [...this.listeners]) listener();
  }

  private end(problem: string | null): void {
    const last = this.last;
    if (last !== null && last.state !== "ended")
      this.ended = {
        ...last,
        state: "ended",
        problem,
        local: null,
        remote: null,
      };
    this.changed();
  }

  private clearOutgoing(): void {
    if (this.outgoing === null) return;
    clearTimeout(this.outgoing.timer);
    this.outgoing = null;
  }

  dispose(): void {
    this.clearOutgoing();
    this.listeners.clear();
  }
}
