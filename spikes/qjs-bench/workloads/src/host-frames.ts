// Host-side frames, encoded with the client's own generated codecs, exactly as
// the core sends them: a handshake ping, then render requests for chat
// messages (V1, the version every product decodes), each closed by a Stop when
// the message leaves the screen.
import {
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_START,
  MESSAGE_TYPE_STOP,
  TRUAPI_CODEC_VERSION,
  VersionedHostHandshakeRequest,
  VersionedProductRendererRenderRequest,
  encodeWireMessage,
  scale,
} from "@parity/truapi";
import { RENDERER_RENDER, SYSTEM_HANDSHAKE } from "@parity/truapi/wire-table";
import type { RenderPayload } from "./bench-worker";

export const RENDER_REQUESTS = 200;
const ROOM_ID = "bench-room";
const MESSAGE_TYPE = "truapi-chat-diagnosis";
const DIAGNOSIS_METHOD_COUNT = 8;

export function hostFrames(): Uint8Array[] {
  const frames: Uint8Array[] = [handshakePing()];
  for (let step = 1; step <= RENDER_REQUESTS; step++) {
    const requestId = `h:${step}`;
    frames.push(renderRequest(requestId, step), stop(requestId));
  }
  return frames;
}

function handshakePing(): Uint8Array {
  return encode("hs:1", SYSTEM_HANDSHAKE, MESSAGE_TYPE_REQUEST,
    VersionedHostHandshakeRequest.enc({ tag: "V1", value: { codecVersion: TRUAPI_CODEC_VERSION } }));
}

export function renderRequest(requestId: string, step: number): Uint8Array {
  const payload: RenderPayload = { step, passed: step % (DIAGNOSIS_METHOD_COUNT + 1) };
  const value = VersionedProductRendererRenderRequest.enc({
    tag: "V1",
    value: {
      context: { tag: "ChatMessage", value: { roomId: ROOM_ID, messageId: `m-${step}`, messageType: MESSAGE_TYPE } },
      payload: scale.bytesToHex(new TextEncoder().encode(JSON.stringify(payload))),
    },
  });
  return encode(requestId, RENDERER_RENDER, MESSAGE_TYPE_START, value);
}

function stop(requestId: string): Uint8Array {
  return encode(requestId, RENDERER_RENDER, MESSAGE_TYPE_STOP, new Uint8Array());
}

function encode(
  requestId: string,
  ids: { trait: number; method: number },
  messageType: number,
  value: Uint8Array,
): Uint8Array {
  const frame = encodeWireMessage({
    requestId,
    payload: { traitId: ids.trait, methodId: ids.method, messageType, value },
  });
  if (frame.isErr()) throw frame.error;
  return frame.value;
}
