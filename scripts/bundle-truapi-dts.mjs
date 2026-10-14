#!/usr/bin/env node
import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const DIST = join(ROOT, "js/packages/truapi/dist");
const OUT_DIR = join(ROOT, "js/packages/truapi/src/playground/codegen");
const OUT = join(OUT_DIR, "truapi-dts.ts");
const HOST_SCRIPT_DTS = join(
  ROOT,
  "rust/crates/truapi-host-cli/js/script-types.d.ts",
);
// neverthrow is hoisted to the workspace root in npm workspaces; fall back to
// the per-package node_modules for non-workspace setups.
async function resolveNeverthrow() {
  for (const candidate of [
    join(ROOT, "node_modules/neverthrow/dist/index.d.ts"),
    join(ROOT, "js/packages/truapi/node_modules/neverthrow/dist/index.d.ts"),
  ]) {
    try {
      await stat(candidate);
      return candidate;
    } catch {
      /* try next */
    }
  }
  throw new Error(
    "bundle-truapi-dts: could not find neverthrow's d.ts under node_modules. Run `npm install` first.",
  );
}
const NEVERTHROW_DTS = await resolveNeverthrow();

// Strip cross-file relative imports — once everything is wrapped in a single
// `declare module "@parity/truapi"` block, relative paths resolve to nothing.
const RELATIVE_IMPORT_RE =
  /^(?:import|export)[^;]*?from\s+["'](\.\.?\/[^"']*)["'];?\s*\n?/gm;
// `import { type Result } from "neverthrow"` lines also need to go: we inline
// neverthrow's real .d.ts content at the top of the bundle, so the symbols
// are in module-local scope and don't need to cross the module boundary.
const NEVERTHROW_IMPORT_RE =
  /^(?:import|export)[^;]*?from\s+["']neverthrow["'];?\s*\n?/gm;

function stripImports(text) {
  return text
    .replace(/\r\n?/g, "\n")
    .replace(RELATIVE_IMPORT_RE, "")
    .replace(NEVERTHROW_IMPORT_RE, "");
}

// The generated client uses `T.<TypeName>` and `S.<HexString|Codec|…>` because
// it imports `* as T from "./types"` and types.d.ts imports `* as S from
// "../scale"`. After stripping those imports, `T` and `S` would be undefined
// — so we re-create them as ambient namespaces wrapping the matching files.
const typeFiles = new Map();
async function collectTypes(path) {
  const rel = relative(DIST, path).replace(/\\/g, "/");
  if (typeFiles.has(rel)) return;
  const source = await readFile(path, "utf8");
  typeFiles.set(rel, path);
  for (const [, specifier] of source.matchAll(RELATIVE_IMPORT_RE)) {
    await collectTypes(
      resolve(dirname(path), specifier.replace(/\.js$/, ".d.ts")),
    );
  }
}
await collectTypes(join(DIST, "index.d.ts"));

async function readFileNamespace(name, relPath) {
  const path = typeFiles.get(relPath);
  if (!path) throw new Error(`bundle: missing ${relPath}`);
  // Drop the `declare` modifier inside the namespace body. Inside a
  // `declare namespace`, ambient-ness is implicit; leaving `declare` causes
  // "modifier not allowed" diagnostics in Monaco's TS worker. Keep the
  // `export` keywords so members are reachable as `T.Foo` / `S.Foo`. Rewrite
  // `S.X` namespace accesses inside the bundle to bare `X` since the
  // referenced symbols all live at module scope (scale.d.ts inlined below).
  const body = stripImports(await readFile(path, "utf8"))
    .replace(/^(\s*export\s+)declare\s+/gm, "$1")
    .replace(/\bS\.([A-Za-z_$][\w$]*)/g, "$1");
  typeFiles.delete(relPath);
  return `declare namespace ${name} {\n${body}\n}\n`;
}

async function readFlat(relPath) {
  const path = typeFiles.get(relPath);
  if (!path) throw new Error(`bundle: missing ${relPath}`);
  const text = stripImports(await readFile(path, "utf8"));
  typeFiles.delete(relPath);
  return text;
}

// scale.d.ts contents live at module scope (its `HexString`, `Codec`, etc.
// are referenced bare from inside the types namespace), then types.d.ts is
// wrapped in `declare namespace T`.
const scaleBody = await readFlat("scale.d.ts");
const namespaceT = await readFileNamespace("T", "generated/types.d.ts");

// The generated types live inside `declare namespace T`, but the real package's
// `index.d.ts` re-exports them at the top level (`export * from "./generated"`).
// That `export *` is dropped when relative imports are stripped, so mirror it
// with one alias per type — otherwise `import { Statement } from "@parity/truapi"`
// resolves against the real package (tsc) but not the Monaco bundle.
const T_EXPORT_RE =
  /\bexport\s+(?:interface|type|const|enum|function|class|namespace)\s+([A-Za-z_$][\w$]*)/g;
const tExports = [...new Set([...namespaceT.matchAll(T_EXPORT_RE)].map((m) => m[1]))];
const namespaceTReExports = tExports
  .map((name) => `export import ${name} = T.${name};`)
  .join("\n");

async function readHostFile(relPath) {
  const path = typeFiles.get(relPath);
  if (!path) throw new Error(`bundle: missing ${relPath}`);
  return stripImports(await readFile(path, "utf8"));
}

const STANDALONE_EXPORT_RE = /^export(?:\s+type)?\s+\{[^}]*\};?\s*\n?/gm;
const hostTransport = (await readHostFile("transport.d.ts")).replace(
  STANDALONE_EXPORT_RE,
  "",
);
const hostClient = (await readHostFile("generated/client.d.ts"))
  .replace(STANDALONE_EXPORT_RE, "")
  .replace(/\bS\.([A-Za-z_$][\w$]*)/g, "$1");

const chunks = [];
for (const [rel, path] of typeFiles) {
  const text = stripImports(await readFile(path, "utf8"));
  chunks.push(`// ${rel}\n${text}`);
}

// Strip the trailing `export { ... }` from neverthrow's d.ts since we're
// inlining it inside `declare module "@parity/truapi"` — re-exporting names
// at the bottom isn't meaningful in that context.
const neverthrow = (await readFile(NEVERTHROW_DTS, "utf8"))
  .replace(/\r\n?/g, "\n")
  .replace(/^export\s+\{[^}]*\};?\s*$/gm, "");

const bundled = [
  "// neverthrow (inlined)",
  neverthrow,
  "// scale.d.ts (module-scope)",
  scaleBody,
  "// generated/types.d.ts (namespace T)",
  namespaceT,
  "// re-export namespace T at module top level (mirrors index.d.ts `export *`)",
  namespaceTReExports,
  ...chunks,
].join("\n\n");

const hostNeverthrow = neverthrow
  .replace(/A extends readonly any\[\]/g, "A extends readonly unknown[]")
  .replace(
    /Fn extends \(\.\.\.args: readonly any\[\]\) => any/g,
    "Fn extends (...args: never[]) => unknown",
  );
const hostScriptDts =
  `// Auto-generated by scripts/bundle-truapi-dts.mjs. Do not edit.

${hostNeverthrow}

type Encoder<T> = (value: T) => Uint8Array;
type Decoder<T> = (value: Uint8Array | ArrayBuffer | string) => T;
type Codec<T> = [Encoder<T>, Decoder<T>] & {
  enc: Encoder<T>;
  dec: Decoder<T>;
};
type ResultPayload<Ok, Err> =
  | { success: true; value: Ok }
  | { success: false; value: Err };
type HexString = \`0x\${string}\`;
type CallErrorValue<DomainError> =
  | { tag: "Domain"; value: DomainError }
  | { tag: "Denied"; value?: undefined }
  | { tag: "Unsupported"; value?: undefined }
  | { tag: "MalformedFrame"; value: { reason: string } }
  | { tag: "HostFailure"; value: { reason: string } }
  | { tag: "Cancelled"; value?: undefined };

${namespaceT}

${hostTransport}

${hostClient}

/** Context injected alongside \`truapi\` by the headless host runner. */
export interface HostContext {
  /** Product id served by the host. */
  productId: string;
  /** Product account for \`derivationIndex\`, which defaults to zero. */
  productAccount(index?: number): T.ProductAccountId;
}

export type ScriptAssert = (condition: unknown, ...message: unknown[]) => asserts condition;
`.replace(/[ \t]+$/gm, "");

function asTemplateLiteral(s) {
  return s.replace(/\\/g, "\\\\").replace(/`/g, "\\`").replace(/\$\{/g, "\\${");
}

await mkdir(OUT_DIR, { recursive: true });
await writeFile(
  OUT,
  `// Auto-generated by scripts/bundle-truapi-dts.mjs. Do not edit.\n` +
    `export const truapiDts = \`${asTemplateLiteral(bundled)}\`;\n`,
);
await writeFile(HOST_SCRIPT_DTS, hostScriptDts);

console.log(
  `wrote ${relative(ROOT, OUT)} and ${relative(ROOT, HOST_SCRIPT_DTS)} (${bundled.length} chars, neverthrow inlined)`,
);
