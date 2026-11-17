import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_CANCEL,
  MESSAGE_TYPE_REQUEST,
  VersionedHostRequestLoginRequest,
} from "@parity/truapi";
import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";
import {
  ACCOUNT_REQUEST_LOGIN,
  ACCOUNT_GET_USER_ID,
} from "@parity/truapi/wire-table";
import { describe, expect, it } from "bun:test";
import {
  createLoginRequests,
  loginRequestId,
  loginResponse,
  type LoginRequestHost,
} from "./login-requests.js";

/** A frame as the product's client sends it. */
function frame(
  requestId: string,
  ids: { trait: number; method: number },
  messageType: number,
  value: Uint8Array,
): Uint8Array {
  return encodeWireMessage({
    requestId,
    payload: {
      traitId: ids.trait,
      methodId: ids.method,
      messageType,
      value,
    },
  })._unsafeUnwrap();
}

const loginRequest = (requestId: string) =>
  frame(
    requestId,
    ACCOUNT_REQUEST_LOGIN,
    MESSAGE_TYPE_REQUEST,
    VersionedHostRequestLoginRequest.enc({
      tag: "V1",
      value: { reason: "to save your notes" },
    }),
  );

/** A reply's value, decoded by the generated table the product's client uses. */
function decodeReply(reply: Uint8Array) {
  const { requestId, payload } = decodeWireMessage(reply)._unsafeUnwrap();
  const decode =
    WIRE_DECODE_TABLE[payload.traitId * 256 + payload.methodId]?.[
      payload.messageType
    ];
  if (!decode) throw new Error("no decoder for this reply");
  return {
    requestId,
    address: [payload.traitId, payload.methodId],
    value: decode(payload.value),
  };
}

describe("login requests the test host answers", () => {
  it("recognises a product's login request by its id", () => {
    expect(loginRequestId(loginRequest("p:7"))).toBe("p:7");
  });

  it("leaves every other frame to the core", () => {
    expect(
      loginRequestId(
        frame(
          "p:1",
          ACCOUNT_GET_USER_ID,
          MESSAGE_TYPE_REQUEST,
          new Uint8Array(),
        ),
      ),
    ).toBeUndefined();
    // The withdrawal of a login request is the core's to handle, not a request.
    expect(
      loginRequestId(
        frame(
          "p:7",
          ACCOUNT_REQUEST_LOGIN,
          MESSAGE_TYPE_CANCEL,
          new Uint8Array(),
        ),
      ),
    ).toBeUndefined();
    expect(loginRequestId(new Uint8Array([1, 2, 3]))).toBeUndefined();
  });

  it("leaves a version it cannot read to the core", () => {
    expect(
      loginRequestId(
        frame(
          "p:7",
          ACCOUNT_REQUEST_LOGIN,
          MESSAGE_TYPE_REQUEST,
          new Uint8Array([1]),
        ),
      ),
    ).toBeUndefined();
  });

  it("answers in the shape the product's client decodes", () => {
    for (const response of [
      "Success",
      "AlreadyConnected",
      "Rejected",
    ] as const) {
      const { requestId, address, value } = decodeReply(
        loginResponse("p:7", response),
      );
      expect(requestId).toBe("p:7");
      expect(address).toEqual([
        ACCOUNT_REQUEST_LOGIN.trait,
        ACCOUNT_REQUEST_LOGIN.method,
      ]);
      expect(value).toEqual({
        success: true,
        value: { tag: "V1", value: response },
      });
    }
  });
});

/** What a reply frame answers, as the product's client reads it. */
function answerOf(reply: Uint8Array): string {
  const answer = decodeReply(reply).value as
    | { success: true; value: { value: string } }
    | { success: false; value: { tag: string } };
  return answer.success ? answer.value.value : answer.value.tag;
}

const cancel = (requestId: string) =>
  frame(
    requestId,
    ACCOUNT_REQUEST_LOGIN,
    MESSAGE_TYPE_CANCEL,
    new Uint8Array(),
  );

/** A host page whose sign-in finishes when the test says so. */
function fakeHost(accounts = ["alice", "bob"]) {
  let signedIn = false;
  const signIns: string[] = [];
  let finish: (failure?: Error) => void = () => {};
  const host: LoginRequestHost = {
    signedIn: () => signedIn,
    accounts: () => accounts,
    signIn(account) {
      signIns.push(account);
      return new Promise<void>((resolve, reject) => {
        finish = (failure) => {
          if (failure) return reject(failure);
          signedIn = true;
          resolve();
        };
      });
    },
  };
  return { host, signIns, finish: (failure?: Error) => finish(failure) };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("answering login requests for the host page", () => {
  it("leaves requests to the core until told how to answer, and while signed in", async () => {
    const { host, finish } = fakeHost();
    const logins = createLoginRequests(host);
    const replies: Uint8Array[] = [];
    expect(logins.handle(loginRequest("p:1"), (r) => replies.push(r))).toBe(
      false,
    );

    logins.set("success");
    logins.handle(loginRequest("p:2"), (r) => replies.push(r));
    finish();
    await flush();
    expect(logins.handle(loginRequest("p:3"), (r) => replies.push(r))).toBe(
      false,
    );
    expect(replies.map(answerOf)).toEqual(["Success"]);
  });

  it("answers Rejected without signing in when told to reject", () => {
    const { host, signIns } = fakeHost();
    const logins = createLoginRequests(host);
    logins.set("reject");
    const replies: Uint8Array[] = [];
    expect(logins.handle(loginRequest("p:1"), (r) => replies.push(r))).toBe(
      true,
    );
    expect(replies.map(answerOf)).toEqual(["Rejected"]);
    expect(signIns).toEqual([]);
  });

  it("signs in once for requests made together, as the named account", async () => {
    const { host, signIns, finish } = fakeHost();
    const logins = createLoginRequests(host);
    logins.set("success", "bob");
    const replies: string[] = [];
    logins.handle(loginRequest("p:1"), (r) => replies.push(answerOf(r)));
    logins.handle(loginRequest("p:2"), (r) => replies.push(answerOf(r)));
    finish();
    await flush();
    expect(signIns).toEqual(["bob"]);
    expect(replies).toEqual(["Success", "AlreadyConnected"]);
  });

  it("answers a withdrawn request Cancelled, once", async () => {
    const { host, finish } = fakeHost();
    const logins = createLoginRequests(host);
    logins.set("success");
    const replies: string[] = [];
    logins.handle(loginRequest("p:1"), (r) => replies.push(answerOf(r)));
    expect(logins.handle(cancel("p:1"), (r) => replies.push(answerOf(r)))).toBe(
      true,
    );
    finish();
    await flush();
    expect(replies).toEqual(["Cancelled"]);
    // A withdrawal of a request this host is not answering is the core's.
    expect(logins.handle(cancel("p:9"), () => {})).toBe(false);
  });

  it("lets a test wait for a sign-in a login request started", async () => {
    const { host, finish } = fakeHost();
    const logins = createLoginRequests(host);
    logins.set("success");
    logins.handle(loginRequest("p:1"), () => {});
    let settled = false;
    const waiting = logins.settled().then(() => (settled = true));
    await flush();
    expect(settled).toBe(false);
    finish();
    await waiting;
    expect(settled).toBe(true);
  });

  it("answers Rejected when the account is gone or the sign-in fails", async () => {
    const accounts = ["alice", "bob"];
    const { host, signIns, finish } = fakeHost(accounts);
    const logins = createLoginRequests(host);
    expect(() => logins.set("success", "carol")).toThrow(/no account "carol"/);

    logins.set("success", "bob");
    accounts.splice(1);
    const replies: string[] = [];
    logins.handle(loginRequest("p:1"), (r) => replies.push(answerOf(r)));
    expect(signIns).toEqual([]);

    logins.set("success");
    logins.handle(loginRequest("p:2"), (r) => replies.push(answerOf(r)));
    finish(new Error("activation failed"));
    await flush();
    expect(replies).toEqual(["Rejected", "Rejected"]);
  });

  it("refuses a behaviour it does not know", () => {
    const logins = createLoginRequests(fakeHost().host);
    expect(() => logins.set("auto" as unknown as "success")).toThrow(
      /takes "success" or "reject"/,
    );
  });
});
