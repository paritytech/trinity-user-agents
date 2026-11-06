import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_CANCEL,
  MESSAGE_TYPE_REQUEST,
  scale,
  VersionedHostRequestLoginError,
  VersionedHostRequestLoginRequest,
  VersionedHostRequestLoginResponse,
} from "@parity/truapi";
import {
  ACCOUNT_REQUEST_LOGIN,
  ACCOUNT_GET_USER_ID,
} from "@parity/truapi/wire-table";
import { describe, expect, it } from "bun:test";
import { loginRequestId, loginResponse } from "./login-requests.js";

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
      const message = decodeWireMessage(
        loginResponse("p:7", response),
      )._unsafeUnwrap();
      expect(message.requestId).toBe("p:7");
      expect([message.payload.traitId, message.payload.methodId]).toEqual([
        ACCOUNT_REQUEST_LOGIN.trait,
        ACCOUNT_REQUEST_LOGIN.method,
      ]);
      // The decoder the generated client applies to this method's response.
      const decoded = scale
        .Result(
          VersionedHostRequestLoginResponse,
          scale.CallError(VersionedHostRequestLoginError),
        )
        .dec(message.payload.value);
      expect(decoded).toEqual({
        success: true,
        value: { tag: "V1", value: response },
      });
    }
  });
});
