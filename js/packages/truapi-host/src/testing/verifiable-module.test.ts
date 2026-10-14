// The browser core does not link `verifiable`: it fetches the module from
// `truapi_verifiable_bg.wasm` beside its own files and checks it against the hash `make wasm`
// compiled in. A wrong path or a stale copy still builds and loads, and fails
// only on the first ring-VRF call, which no product call reaches here without a
// chain. So this checks each bundle's layout and pin, and drives the testing
// core's own load through its test-host export.
import { describe, expect, it } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { wasmArtifact, wasmIsBuilt } from "./require-wasm.js";

const suite = wasmIsBuilt(
  "testing/truapi_server.js",
  "testing/truapi_verifiable.js",
)
  ? describe
  : describe.skip;

const bundles = ["web", "testing"];

const layoutSuite = wasmIsBuilt(
  ...bundles.flatMap((bundle) => [
    `${bundle}/truapi_server_bg.wasm`,
    `${bundle}/truapi_verifiable_bg.wasm`,
  ]),
)
  ? describe
  : describe.skip;

async function glue<T>(
  relativePath: string,
): Promise<T & { default(): Promise<unknown> }> {
  const module = (await import(
    pathToFileURL(wasmArtifact(relativePath)).href
  )) as T & { default(): Promise<unknown> };
  await module.default();
  return module;
}

suite("verifiable module", () => {
  it("loads beside the testing core and derives the member it derives directly", async () => {
    const core = await glue<{
      ringVrfMember(entropy: Uint8Array): Promise<Uint8Array>;
    }>("testing/truapi_server.js");
    const verifiable = await glue<{ member(entropy: Uint8Array): Uint8Array }>(
      "testing/truapi_verifiable.js",
    );
    const entropy = new Uint8Array(32).fill(4);

    // `member` answers a SCALE `Result<[u8; 32], String>`: `0x00`, then the key.
    const direct = verifiable.member(entropy);
    expect(direct[0]).toBe(0);
    expect(await core.ringVrfMember(entropy)).toEqual(direct.subarray(1));
  });
});

layoutSuite("verifiable module in each bundle", () => {
  for (const bundle of bundles) {
    it(`${bundle}: the core pins the module shipped beside it`, () => {
      const digest = createHash("sha256")
        .update(
          readFileSync(wasmArtifact(`${bundle}/truapi_verifiable_bg.wasm`)),
        )
        .digest("hex");
      const core = readFileSync(
        wasmArtifact(`${bundle}/truapi_server_bg.wasm`),
      );
      expect(core.includes(digest)).toBe(true);
    });

    it(`${bundle}: the core's loader resolves to the module`, () => {
      const snippets = wasmArtifact(`${bundle}/snippets`);
      const loaders = readdirSync(snippets, { recursive: true })
        .map((entry) => join(snippets, String(entry)))
        .filter((path) => path.endsWith(".js"))
        .filter((path) =>
          readFileSync(path, "utf8").includes("truapi_verifiable_bg.wasm"),
        );
      expect(loaders).toHaveLength(1);

      const source = readFileSync(loaders[0], "utf8");
      const targets = [
        ...source.matchAll(/new URL\("([^"]+)", import\.meta\.url\)/g),
      ].map(([, relative]) =>
        fileURLToPath(new URL(relative, pathToFileURL(loaders[0]))),
      );
      expect(targets).toEqual([
        wasmArtifact(`${bundle}/truapi_verifiable_bg.wasm`),
        wasmArtifact(`${bundle}/truapi_verifiable.js`),
      ]);
      expect(targets.every((target) => existsSync(target))).toBe(true);
    });
  }
});
