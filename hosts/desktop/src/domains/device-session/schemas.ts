/**
 * Wire boundary for the device-to-device signaling session.
 *
 * `SignalingStatementData` mirrors the two generic variants of Android's
 * `StructuredStatementData` sealed class (and chat's codec in
 * `chat/p2p/requests/schemas.ts`):
 *   @EnumIndex(0) Request  { requestId, messages: [bytes] }
 *   @EnumIndex(1) Response { requestId, responseCode }
 *
 * The device session speaks ONLY these two. The chat-specific `MultiRequest` /
 * `MultiResponse` variants (@EnumIndex 2/3) carry chat vocabulary and are
 * intentionally absent here — device-session must not depend on chat internals.
 *
 * INVARIANT: the SCALE Enum discriminants MUST stay `Request = 0`, `Response = 1`
 * so this codec is byte-identical to chat's / Android's on the wire. The
 * discriminant is the variant index; do not reorder. `schemas.spec.ts` pins the
 * leading byte (0x00 / 0x01) to guard against drift.
 */

import { Bytes, Enum, Struct, Vector, str, u8 } from 'scale-ts';

const SignalingRequest = Struct({
  requestId: str,
  messages: Vector(Bytes()),
});

const SignalingResponse = Struct({
  requestId: str,
  responseCode: u8,
});

export const SignalingStatementData = Enum({
  Request: SignalingRequest,
  Response: SignalingResponse,
});
