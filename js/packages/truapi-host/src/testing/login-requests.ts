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
  MESSAGE_TYPE_CANCEL,
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

/** Whether `frame` is addressed to `account_request_login` as `messageType`. */
function loginFrameId(
  frame: Uint8Array,
  messageType: number,
): string | undefined {
  const message = decodeWireMessage(frame);
  if (message.isErr()) return undefined;
  const { requestId, payload } = message.value;
  return payload.traitId === ACCOUNT_REQUEST_LOGIN.trait &&
    payload.methodId === ACCOUNT_REQUEST_LOGIN.method &&
    payload.messageType === messageType
    ? requestId
    : undefined;
}

/**
 * The request id of a `V1` login request, or `undefined` for any other frame.
 *
 * Only the version this host can answer is claimed. A later version falls
 * through to the core, which knows it, rather than being misread here.
 */
export function loginRequestId(frame: Uint8Array): string | undefined {
  const requestId = loginFrameId(frame, MESSAGE_TYPE_REQUEST);
  if (requestId === undefined) return undefined;
  try {
    const { payload } = decodeWireMessage(frame)._unsafeUnwrap();
    const request = VersionedHostRequestLoginRequest.dec(payload.value);
    return request.tag === "V1" ? requestId : undefined;
  } catch {
    return undefined;
  }
}

/** The request id a login request's withdrawal names, or `undefined`. */
export function loginCancelId(frame: Uint8Array): string | undefined {
  return loginFrameId(frame, MESSAGE_TYPE_CANCEL);
}

/**
 * The frame answering the login request `requestId`: `response`, or the
 * `Cancelled` call error the core answers a withdrawn call with.
 */
export function loginResponse(
  requestId: string,
  response: HostRequestLoginResponse | "Cancelled",
): Uint8Array {
  const frame = encodeWireMessage({
    requestId,
    payload: {
      traitId: ACCOUNT_REQUEST_LOGIN.trait,
      methodId: ACCOUNT_REQUEST_LOGIN.method,
      messageType: MESSAGE_TYPE_RESPONSE,
      value: LoginResult.enc(
        response === "Cancelled"
          ? { success: false, value: { tag: "Cancelled" } }
          : { success: true, value: { tag: "V1", value: response } },
      ),
    },
  });
  if (frame.isErr()) throw frame.error;
  return frame.value;
}

/** What the login answerer needs from the host page. */
export interface LoginRequestHost {
  /** Whether a session is active. */
  signedIn(): boolean;
  /** Names of the accounts the host can sign in as, in order. */
  accounts(): string[];
  /** Activate a session as `account`. */
  signIn(account: string): Promise<void>;
}

/** Answers login requests on behalf of the host page. */
export interface LoginRequests {
  /** Answer requests made while signed out with `behavior` from now on. */
  set(behavior: LoginRequestBehavior, account?: string): void;
  /** Leave login requests to the core again. */
  clear(): void;
  /**
   * Answer `frame` when it is a login request, or the withdrawal of one, that
   * this host is answering, and report whether it did. Anything else goes on
   * to the core.
   */
  handle(frame: Uint8Array, reply: (frame: Uint8Array) => void): boolean;
  /**
   * Resolves once a sign-in started for a login request has finished, so a
   * test changing the session afterwards does not race it.
   */
  settled(): Promise<void>;
}

/** Build the login answerer for one host page. */
export function createLoginRequests(host: LoginRequestHost): LoginRequests {
  let answer: { behavior: LoginRequestBehavior; account?: string } | undefined;
  // One sign-in at a time: a second product asking while the first is being
  // signed in waits for it, rather than activating again and invalidating the
  // first product's session.
  let signingIn: Promise<void> | undefined;
  /** Withdraw an answered request, by id, while its sign-in runs. */
  const pending = new Map<string, () => void>();

  const unknownAccount = (account: string) =>
    `no account "${account}" on this host; have ${host.accounts().join(", ")}`;

  return {
    set(behavior, account) {
      if (behavior !== "success" && behavior !== "reject") {
        throw new Error(
          `testHost.setLoginBehavior takes "success" or "reject", got ` +
            `${JSON.stringify(behavior)}. Whether the host starts signed in ` +
            'is the `loginBehavior: "auto" | "manual"` fixture option.',
        );
      }
      if (account !== undefined && !host.accounts().includes(account)) {
        throw new Error(unknownAccount(account));
      }
      answer = { behavior, account };
    },
    clear() {
      answer = undefined;
    },
    handle(frame, reply) {
      if (pending.size > 0) {
        const withdrawn = loginCancelId(frame);
        const withdraw =
          withdrawn === undefined ? undefined : pending.get(withdrawn);
        if (withdraw) {
          withdraw();
          return true;
        }
      }
      if (!answer || host.signedIn()) return false;
      const requestId = loginRequestId(frame);
      if (requestId === undefined) return false;
      if (answer.behavior === "reject") {
        reply(loginResponse(requestId, "Rejected"));
        return true;
      }
      const account = answer.account ?? host.accounts()[0];
      if (account === undefined || !host.accounts().includes(account)) {
        console.error(
          "test host could not answer a login request:",
          account === undefined
            ? "it has no accounts"
            : unknownAccount(account),
        );
        reply(loginResponse(requestId, "Rejected"));
        return true;
      }
      const owner = signingIn === undefined;
      signingIn ??= host.signIn(account).finally(() => {
        signingIn = undefined;
      });
      let answered = false;
      const settle = (response: HostRequestLoginResponse | "Cancelled") => {
        if (answered) return;
        answered = true;
        pending.delete(requestId);
        reply(loginResponse(requestId, response));
      };
      // A withdrawal answers `Cancelled` at once, as the core does. A sign-in
      // already under way still completes.
      pending.set(requestId, () => settle("Cancelled"));
      signingIn.then(
        () => settle(owner ? "Success" : "AlreadyConnected"),
        (error: unknown) => {
          console.error(
            "test host could not sign in for a login request",
            error,
          );
          settle("Rejected");
        },
      );
      return true;
    },
    settled: () => (signingIn ?? Promise.resolve()).catch(() => {}),
  };
}
