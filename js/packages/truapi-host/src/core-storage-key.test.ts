import { expect, test } from "bun:test";
import { CoreStorageKey } from "./generated/host-callbacks.js";

test("permission storage codecs preserve Rust's shared label scopes", () => {
  const encoded = new Uint8Array([
    2, 24, 99, 97, 108, 108, 101, 114, 4, 24, 116, 97, 114, 103, 101, 116, 2,
  ]);
  const key: CoreStorageKey = {
    tag: "PermissionAuthorization",
    value: {
      scope: "caller",
      request: {
        tag: "CrossProductSignature",
        value: { target: "target", kind: "Raw" },
      },
    },
  };
  expect({
    decoded: CoreStorageKey.dec(encoded),
    encoded: CoreStorageKey.enc(key),
  }).toEqual({ decoded: key, encoded });
});
