// The build copies a JavaScript project keeps of its stacks, as docs/design/trinity-stack-library.md
// describes: `.trinity/` holds each stack's signed copy and dotNS record, and a module that
// bundles them, all committed with the project.
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Pointer } from "../wasm/trinity_stack.js";
import {
  type DotnsRecord,
  type DotnsTransport,
  decodeDotnsRecord,
  encodeDotnsRecord,
  readDotns,
} from "./dotns.js";
import {
  type Descriptor,
  type Fetch,
  accept,
  decodeLink,
  encodeLink,
  fetchDescriptor,
  parse,
  stackId,
} from "./index.js";

/** Where a project's stacks live, and how to reach them. */
export type ProjectOptions = {
  /** `.trinity` by default. */
  dir?: string;
  fetch?: Fetch;
  timeoutMs?: number;
  transport?: (descriptor: Descriptor) => DotnsTransport;
  now?: Date;
};

/** What a command wrote. `update` exits 2 when it wrote anything, so CI can open a pull request. */
export type Outcome = { changed: string[] };

const NAME = /^[a-z0-9][a-z0-9-]*$/;

/**
 * Adds a stack under `name` from a link, or a URL and its key, then writes its copies as
 * `update` does. A build copy must be signed, so a stack without a key is refused.
 */
export async function add(
  name: string,
  target: string,
  key: string | undefined,
  options: ProjectOptions = {},
): Promise<Outcome> {
  if (!NAME.test(name))
    throw new Error(`${name} is not a stack name: use a-z, 0-9 and -`);
  const linked = target.startsWith("https://")
    ? { url: target }
    : decodeLink(target);
  if (key && linked.key && key !== linked.key)
    throw new Error(`the link carries key ${linked.key}, not ${key}`);
  const pointer: Pointer = { url: linked.url, key: key ?? linked.key };
  stackId(pointer);
  const stacks = await readStacks(options);
  if (stacks[name])
    throw new Error(`${name} is already added; remove it first`);
  const changed = await refresh(name, pointer, options);
  await writeStacks({ ...stacks, [name]: encodeLink(pointer) }, options);
  return { changed: [...changed, "stacks.json", ...(await generate(options))] };
}

/**
 * Fetches each stack, or those `names` lists, and writes what moved: a newer signed copy, and
 * its dotNS record read at Asset Hub's latest finalized block. Refuses an unsigned or expired
 * stack, and anything that would lower `sequence`. A stack that fails keeps its copies and is
 * named in `failures`, while the others still update.
 */
export async function update(
  names: string[],
  options: ProjectOptions = {},
): Promise<Outcome & { failures: string[] }> {
  const stacks = await readStacks(options);
  const unknown = names.filter((name) => !stacks[name]);
  if (unknown.length > 0)
    throw new Error(`no stack named ${unknown.join(", ")}`);
  const changed: string[] = [];
  const failures: string[] = [];
  for (const name of names.length > 0 ? names : Object.keys(stacks)) {
    try {
      changed.push(...(await refresh(name, decodeLink(stacks[name]), options)));
    } catch (error) {
      failures.push(
        `${name}: ${error instanceof Error ? error.message : error}`,
      );
    }
  }
  changed.push(...(await generate(options)));
  return { changed, failures };
}

/** Each stack's name, link and the copy the project holds. */
export async function list(options: ProjectOptions = {}) {
  const stacks = await readStacks(options);
  return Promise.all(
    Object.entries(stacks).map(async ([name, link]) => {
      const document = await readOptional(join(dir(options), `${name}.json`));
      return {
        name,
        link,
        descriptor: document && parse(document).descriptor,
        dotns: await readRecord(name, options),
      };
    }),
  );
}

/** Removes a stack and its copies. */
export async function remove(
  name: string,
  options: ProjectOptions = {},
): Promise<Outcome> {
  const { [name]: removed, ...stacks } = await readStacks(options);
  if (!removed) throw new Error(`no stack named ${name}`);
  await writeStacks(stacks, options);
  for (const file of [`${name}.json`, `${name}.dotns.json`])
    await rm(join(dir(options), file), { force: true });
  return { changed: ["stacks.json", ...(await generate(options))] };
}

/** Fetches one stack and writes its copies when they moved; returns the files it wrote. */
async function refresh(
  name: string,
  pointer: Pointer,
  options: ProjectOptions,
): Promise<string[]> {
  if (!pointer.key)
    throw new Error("a build copy must be signed; add the stack with its key");
  const documentFile = `${name}.json`;
  const committed = await readOptional(join(dir(options), documentFile));
  const accepted = accept(
    pointer,
    committed,
    await fetchDescriptor(pointer.url, options),
    options.now,
  );
  const refusals: Partial<Record<string, string>> = {
    unreachable: `${pointer.url} is unreachable`,
    refused: `${pointer.url} serves a descriptor that breaks a rule`,
    rollback: `${pointer.url} serves a lower sequence than the committed copy`,
    conflict: `${pointer.url} serves other content under the committed sequence`,
  };
  const refusal = refusals[accepted.status];
  if (refusal) throw new Error(refusal);
  if (accepted.expired)
    throw new Error(`the stack expired at ${accepted.descriptor.expiresAt}`);
  // Read before writing anything, so a stack whose Asset Hub is down keeps its copies as they were.
  const record = accepted.descriptor.chains.some(
    (chain) => chain.role === "AssetHub",
  )
    ? await readDotns(accepted.descriptor, {
        transport: options.transport?.(accepted.descriptor),
        fetch: options.fetch,
        timeoutMs: options.timeoutMs,
      })
    : undefined;
  const changed: string[] = [];
  if (!committed || !same(committed, accepted.document)) {
    await write(documentFile, accepted.document, options);
    changed.push(documentFile);
  }
  const recordFile = `${name}.dotns.json`;
  const kept = await readRecord(name, options);
  if (!record) {
    if (await readOptional(join(dir(options), recordFile))) {
      await rm(join(dir(options), recordFile));
      changed.push(recordFile);
    }
  } else if (
    // A record of the same contracts stays as committed, so a new block alone is no change.
    kept?.generation !== record.generation ||
    JSON.stringify(kept.dotns) !== JSON.stringify(record.dotns)
  ) {
    await write(recordFile, encodeDotnsRecord(record), options);
    changed.push(recordFile);
  }
  return changed;
}

/** Writes `index.js` and `index.d.ts` for the stacks the project holds; returns those it changed. */
async function generate(options: ProjectOptions): Promise<string[]> {
  const stacks = await readStacks(options);
  const entries = await Promise.all(
    Object.entries(stacks).map(async ([name, link]) => {
      const document = await readOptional(join(dir(options), `${name}.json`));
      if (!document) return undefined;
      const value = {
        pointer: decodeLink(link),
        document: new TextDecoder().decode(document),
        dotns: (await readRecord(name, options)) ?? null,
      };
      return [name, value] as const;
    }),
  );
  const bundled = entries.filter((entry) => entry !== undefined);
  const header =
    "// Generated by `trinity-stack update` from stacks.json. Do not edit.\n";
  const module =
    header +
    "export const stacks = {\n" +
    bundled
      .map(
        ([name, value]) =>
          `  ${JSON.stringify(name)}: ${JSON.stringify(value)},\n`,
      )
      .join("") +
    "};\n";
  const declarations =
    header +
    'import type { Bundled } from "@parity/trinity-stack/reader";\n\n' +
    "export declare const stacks: {\n" +
    bundled
      .map(([name]) => `  readonly ${JSON.stringify(name)}: Bundled;\n`)
      .join("") +
    "};\n";
  const changed: string[] = [];
  for (const [file, text] of [
    ["index.js", module],
    ["index.d.ts", declarations],
  ] as const) {
    const bytes = new TextEncoder().encode(text);
    const existing = await readOptional(join(dir(options), file));
    if (!existing || !same(existing, bytes)) {
      await write(file, bytes, options);
      changed.push(file);
    }
  }
  return changed;
}

function dir(options: ProjectOptions) {
  return options.dir ?? ".trinity";
}

async function readStacks(
  options: ProjectOptions,
): Promise<Record<string, string>> {
  const bytes = await readOptional(join(dir(options), "stacks.json"));
  return bytes ? JSON.parse(new TextDecoder().decode(bytes)) : {};
}

async function writeStacks(
  stacks: Record<string, string>,
  options: ProjectOptions,
) {
  const sorted = Object.fromEntries(
    Object.entries(stacks).sort(([a], [b]) => a.localeCompare(b)),
  );
  await write(
    "stacks.json",
    new TextEncoder().encode(`${JSON.stringify(sorted, null, 2)}\n`),
    options,
  );
}

/** The committed dotNS record, or undefined when there is none or it no longer reads. */
async function readRecord(
  name: string,
  options: ProjectOptions,
): Promise<DotnsRecord | undefined> {
  const bytes = await readOptional(join(dir(options), `${name}.dotns.json`));
  try {
    return bytes && decodeDotnsRecord(bytes);
  } catch {
    return undefined;
  }
}

async function readOptional(path: string): Promise<Uint8Array | undefined> {
  try {
    return new Uint8Array(await readFile(path));
  } catch (error) {
    if ((error as { code?: string }).code === "ENOENT") return undefined;
    throw error;
  }
}

async function write(file: string, bytes: Uint8Array, options: ProjectOptions) {
  await mkdir(dir(options), { recursive: true });
  await writeFile(join(dir(options), file), bytes);
}

function same(a: Uint8Array, b: Uint8Array) {
  return a.length === b.length && a.every((byte, index) => byte === b[index]);
}
