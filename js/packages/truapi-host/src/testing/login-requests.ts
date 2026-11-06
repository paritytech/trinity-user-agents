// A product's login request, answered by the test host page itself.
//
// The test host runs the core's signing host, which answers a login request
// with `Rejected` whenever no session is active: signing in belongs to the
// app that embeds it. A suite driving a product's cold login path needs the
// request approved, the way a user approves it on a real host. Every product
// frame crosses the host page on its way to the core, so the page recognises
// this one request there and answers it, and every other frame reaches the
// core untouched.
//
// Encoded and decoded with `@parity/truapi`'s generated codecs, the same ones
// the product's client decodes the answer with.

import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  scale,
  VersionedHostRequestLoginError,
  VersionedHostRequestLoginRequest,
  VersionedHostRequestLoginResponse,
  type HostRequestLoginResponse,
} from "@parity/truapi";
import { ACCOUNT_REQUEST_LOGIN } from "@parity/truapi/wire-table";

/**
 * How the test host answers a product's login request while signed out.
 * `"success"` signs an account in and answers `Success`. `"reject"` answers
 * `Rejected`, as a user dismissing the login would.
 */
export type LoginRequestBehavior = "success" | "reject";

const LoginResult = scale.Result(
  VersionedHostRequestLoginResponse,
  scale.CallError(VersionedHostRequestLoginError),
);

/**
 * The request id of a `V1` login request, or `undefined` for any other frame.
 *
 * Only the version this host can answer is claimed. A later version falls
 * through to the core, which knows it, rather than being misread here.
 */
export function loginRequestId(frame: Uint8Array): string | undefined {
  const message = decodeWireMessage(frame);
  if (message.isErr()) return undefined;
  const { requestId, payload } = message.value;
  if (
    payload.traitId !== ACCOUNT_REQUEST_LOGIN.trait ||
    payload.methodId !== ACCOUNT_REQUEST_LOGIN.method ||
    payload.messageType !== MESSAGE_TYPE_REQUEST
  ) {
    return undefined;
  }
  try {
    const request = VersionedHostRequestLoginRequest.dec(payload.value);
    return request.tag === "V1" ? requestId : undefined;
  } catch {
    return undefined;
  }
}

/** The frame answering the login request `requestId` with `response`. */
export function loginResponse(
  requestId: string,
  response: HostRequestLoginResponse,
): Uint8Array {
  const frame = encodeWireMessage({
    requestId,
    payload: {
      traitId: ACCOUNT_REQUEST_LOGIN.trait,
      methodId: ACCOUNT_REQUEST_LOGIN.method,
      messageType: MESSAGE_TYPE_RESPONSE,
      value: LoginResult.enc({
        success: true,
        value: { tag: "V1", value: response },
      }),
    },
  });
  if (frame.isErr()) throw frame.error;
  return frame.value;
}
