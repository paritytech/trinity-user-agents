import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

test("playground declarations expose the SDK without container or debugger internals", (t) => {
  const root = mkdtempSync(join(tmpdir(), "truapi-dts-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (path, contents) => {
    const target = join(root, path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, contents);
  };
  const dist = "js/packages/truapi/dist/";
  const declarations = {
    "index.d.ts":
      'export * from "./generated/index.js";\nexport * from "./transport.js";\nexport * as scale from "./scale.js";\n',
    "transport.d.ts":
      "export interface WireProvider { postMessage(frame: Uint8Array): void; }\n",
    "scale.d.ts": "export type Codec<T> = { value: T };\n",
    "generated/index.d.ts":
      'export * from "./types.js";\nexport * from "./client.js";\n',
    "generated/types.d.ts":
      'import * as S from "../scale.js";\nexport interface RemotePermissionRequest { domain: string; }\n',
    "generated/client.d.ts":
      'import * as T from "./types.js";\nexport declare function requestRemotePermission(request: T.RemotePermissionRequest): void;\n',
    "generated/internal.d.ts":
      'import * as T from "./types.js";\nexport declare function authorizeRemotePermission(request: T.RemotePermissionRequest): void;\n',
    "generated/wire-table.d.ts":
      "export declare const PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION: { trait: 8; method: 2 };\n",
    "generated/wire-decode.d.ts":
      "export declare const WIRE_DECODE_TABLE: Record<number, unknown>;\n",
    "playground/codegen/services.d.ts":
      "export declare const services: unknown[];\n",
  };
  for (const [path, source] of Object.entries(declarations)) {
    write(dist + path, source);
  }
  write("node_modules/neverthrow/dist/index.d.ts", "export {};\n");
  mkdirSync(join(root, "rust/crates/truapi-host-cli/js"), { recursive: true });
  write("scripts/bundle-truapi-dts.mjs", "");
  const script = join(root, "scripts/bundle-truapi-dts.mjs");
  copyFileSync(new URL("../bundle-truapi-dts.mjs", import.meta.url), script);
  execFileSync(process.execPath, [script], { stdio: "pipe" });
  const output = readFileSync(
    join(root, "js/packages/truapi/src/playground/codegen/truapi-dts.ts"),
    "utf8",
  );
  const expected = {
    "export import RemotePermissionRequest = T.RemotePermissionRequest": true,
    "function requestRemotePermission": true,
    "type Codec<T>": true,
    authorizeRemotePermission: false,
    PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION: false,
    WIRE_DECODE_TABLE: false,
    "const services": false,
  };
  assert.deepEqual(
    Object.fromEntries(
      Object.keys(expected).map((name) => [name, output.includes(name)]),
    ),
    expected,
  );
});
