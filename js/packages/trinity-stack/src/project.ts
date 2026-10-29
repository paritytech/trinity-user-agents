// The build copies a JavaScript project keeps of its stacks, as docs/design/trinity-stack-library.md
// describes: `.trinity/` holds each stack's signed copy and a module that bundles them, all
// committed with the project.
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Pointer } from "../wasm/trinity_stack.js";
import {
  type Accepted,
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
  now?: Date;
};

/** What a command wrote. `update` exits 2 when it wrote anything, so CI can open a pull request. */
export type Outcome = { changed: string[] };

const NAME = /^[a-z0-9][a-z0-9-]*$/;

/**
 * Adds a stack under `name` from a link, or a URL and its key, then writes its copy as
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
  if (name === "stacks")
    throw new Error("stacks is the name of stacks.json; pick another");
  const linked = new URL(target).searchParams.has("url")
    ? decodeLink(target)
    : { url: target };
  if (key && linked.key && key !== linked.key)
    throw new Error(`the link carries key ${linked.key}, not ${key}`);
  const pointer: Pointer = { url: linked.url, key: key ?? linked.key };
  stackId(pointer);
  const stacks = await readStacks(options);
  if (Object.hasOwn(stacks, name))
    throw new Error(`${name} is already added; remove it first`);
  const { written } = await refresh(name, pointer, options);
  await writeStacks({ ...stacks, [name]: encodeLink(pointer) }, options);
  return { changed: [...written, "stacks.json", ...(await generate(options))] };
}

/**
 * Fetches each stack, or those `names` lists, and writes a newer signed copy. Refuses an unsigned
 * or expired stack, and anything that would lower `sequence`. A stack that fails keeps its copy
 * and is named in `failures`, while the others still update. A copy past `validUntil` is written
 * and named in `warnings`. A project with no stacks writes nothing.
 */
export async function update(
  names: string[],
  options: ProjectOptions = {},
): Promise<Outcome & { failures: string[]; warnings: string[] }> {
  const stacks = await readStacks(options);
  const unknown = names.filter((name) => !Object.hasOwn(stacks, name));
  if (unknown.length > 0)
    throw new Error(`no stack named ${unknown.join(", ")}`);
  const chosen = names.length > 0 ? names : Object.keys(stacks);
  if (chosen.length === 0) return { changed: [], failures: [], warnings: [] };
  const results = await Promise.allSettled(
    chosen.map(async (name) =>
      refresh(name, decodeLink(stacks[name]), options),
    ),
  );
  const changed = results.flatMap((result) =>
    result.status === "fulfilled" ? result.value.written : [],
  );
  const warnings = results.flatMap((result, index) =>
    result.status === "fulfilled" && result.value.stale
      ? [`${chosen[index]}: the copy is past validUntil`]
      : [],
  );
  const failures = results.flatMap((result, index) =>
    result.status === "rejected"
      ? [`${chosen[index]}: ${messageOf(result.reason)}`]
      : [],
  );
  changed.push(...(await generate(options)));
  return { changed, failures, warnings };
}

/** Each stack's name, link and the copies the project holds. */
export async function list(options: ProjectOptions = {}) {
  return (await readCopies(options)).map(({ document, ...copy }) => ({
    ...copy,
    descriptor: document && parse(document).descriptor,
  }));
}

/** Removes a stack and its copy. */
export async function remove(
  name: string,
  options: ProjectOptions = {},
): Promise<Outcome> {
  const stacks = await readStacks(options);
  if (!Object.hasOwn(stacks, name)) throw new Error(`no stack named ${name}`);
  delete stacks[name];
  await writeStacks(stacks, options);
  await rm(join(dir(options), fileOf(name)), { force: true });
  return { changed: ["stacks.json", ...(await generate(options))] };
}

/** Fetches one stack and writes its copy when it moved; returns the files it wrote. */
async function refresh(
  name: string,
  pointer: Pointer,
  options: ProjectOptions,
): Promise<{ written: string[]; stale: boolean }> {
  if (!pointer.key)
    throw new Error("a build copy must be signed; add the stack with its key");
  const file = fileOf(name);
  const accepted = accept(
    pointer,
    await read(file, options),
    await fetchDescriptor(pointer.url, options),
    options.now,
  );
  const refusals: Partial<Record<Accepted["status"], string>> = {
    unreachable: `${pointer.url} is unreachable`,
    refused: `${pointer.url} serves a descriptor that breaks ${"refused" in accepted ? accepted.refused : "a rule"}`,
    rollback: `${pointer.url} serves a lower sequence than the committed copy`,
    conflict: `${pointer.url} serves other content under the committed sequence`,
  };
  const refusal = refusals[accepted.status];
  if (refusal) throw new Error(refusal);
  if (accepted.expired)
    throw new Error(`the stack expired at ${accepted.descriptor.expiresAt}`);
  if (accepted.status !== "updated")
    return { written: [], stale: accepted.stale };
  await write(file, accepted.document, options);
  return { written: [file], stale: accepted.stale };
}

/** Writes `index.js` and `index.d.ts` for the stacks the project holds; returns those it changed. */
async function generate(options: ProjectOptions): Promise<string[]> {
  const bundled = (await readCopies(options)).flatMap(
    ({ name, link, document }) =>
      document
        ? [
            {
              name,
              value: {
                pointer: decodeLink(link),
                document: new TextDecoder().decode(document),
              },
            },
          ]
        : [],
  );
  const header =
    "// Generated by `trinity-stack update` from stacks.json. Do not edit.\n";
  const module =
    header +
    "export const stacks = {\n" +
    bundled
      .map(
        ({ name, value }) =>
          `  ${JSON.stringify(name)}: ${JSON.stringify(value)},\n`,
      )
      .join("") +
    "};\n";
  const declarations =
    header +
    'import type { Bundled } from "@parity/trinity-stack/reader";\n\n' +
    "export declare const stacks: {\n" +
    bundled
      .map(({ name }) => `  readonly ${JSON.stringify(name)}: Bundled;\n`)
      .join("") +
    "};\n";
  const changed: string[] = [];
  for (const [file, text] of [
    ["index.js", module],
    ["index.d.ts", declarations],
  ] as const) {
    const bytes = new TextEncoder().encode(text);
    const existing = await read(file, options);
    if (!existing || !same(existing, bytes)) {
      await write(file, bytes, options);
      changed.push(file);
    }
  }
  return changed;
}

function fileOf(name: string) {
  return `${name}.json`;
}

/** Each stack's link and signed copy, as the project holds them. */
async function readCopies(options: ProjectOptions) {
  return Promise.all(
    Object.entries(await readStacks(options)).map(async ([name, link]) => ({
      name,
      link,
      document: await read(fileOf(name), options),
    })),
  );
}

function dir(options: ProjectOptions) {
  return options.dir ?? ".trinity";
}

async function readStacks(
  options: ProjectOptions,
): Promise<Record<string, string>> {
  const bytes = await read("stacks.json", options);
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

/** A file in the project's stack directory, or undefined when there is none. */
async function read(
  file: string,
  options: ProjectOptions,
): Promise<Uint8Array | undefined> {
  try {
    return new Uint8Array(await readFile(join(dir(options), file)));
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

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
