/**
 * Console arguments as bounded text, for the worker console the developer
 * tools show. It reads values the way a console does, but within fixed
 * budgets, and does not run the logged objects' own code:
 *
 * - a property defined by a getter shows as `[Getter]` and is not read;
 * - an Error's name, message and stack are read, as a console reads them;
 * - sizes of binary data, maps, sets and dates are read through the
 *   built-in accessors captured when this module loads, not the value's;
 * - nesting stops at {@link MAX_DEPTH}, a container lists at most
 *   {@link MAX_ENTRIES} entries, a string keeps {@link MAX_STRING}
 *   characters, and a line stops at {@link MAX_LINE}, before the rest of
 *   the value is visited;
 * - a value that refers back to one it is inside shows as `[Circular]`.
 *
 * A Proxy still runs its own traps when its keys are listed: that cannot be
 * told apart from an ordinary object without running them.
 */

export const MAX_DEPTH = 3;
export const MAX_ENTRIES = 20;
export const MAX_STRING = 500;
export const MAX_LINE = 4000;
/** The most arguments of one call that are described. */
export const MAX_ARGS = 10;

function builtinGetter(
  owner: object | undefined,
  key: string,
): ((this: unknown) => unknown) | null {
  try {
    if (owner === undefined) return null;
    return (Object.getOwnPropertyDescriptor(owner, key)?.get ?? null) as (
      this: unknown,
    ) => unknown;
  } catch {
    return null;
  }
}

const typedArrayPrototype = Object.getPrototypeOf(
  Uint8Array.prototype,
) as object;
const arrayBufferBytes = builtinGetter(ArrayBuffer.prototype, "byteLength");
const viewBytes = builtinGetter(typedArrayPrototype, "byteLength");
const dataViewBytes = builtinGetter(DataView.prototype, "byteLength");
const mapSize = builtinGetter(Map.prototype, "size");
const setSize = builtinGetter(Set.prototype, "size");
const blobSize = builtinGetter(
  typeof Blob === "undefined" ? undefined : Blob.prototype,
  "size",
);
const nodeName = builtinGetter(
  typeof Node === "undefined" ? undefined : Node.prototype,
  "nodeName",
);
const dateTime = Date.prototype.getTime;
const dateIso = Date.prototype.toISOString;

const call = (getter: ((this: unknown) => unknown) | null, value: unknown) =>
  getter === null ? "?" : String(Reflect.apply(getter, value, []));

/** A data property's value, or undefined for a getter or a missing key. */
function dataValue(value: object, key: PropertyKey): unknown {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor !== undefined && "value" in descriptor
    ? descriptor.value
    : undefined;
}

/** The name of the class that made `value`, read without running its code. */
function className(value: object): string {
  try {
    const prototype = Object.getPrototypeOf(value) as object | null;
    if (prototype === null) return "Object";
    const constructor = dataValue(prototype, "constructor");
    const name =
      typeof constructor === "function"
        ? dataValue(constructor, "name")
        : undefined;
    return typeof name === "string" && name !== "" ? name : "Object";
  } catch {
    return "Object";
  }
}

const cut = (text: string, limit: number): string =>
  text.length <= limit
    ? text
    : `${text.slice(0, limit)}… (${text.length - limit} more)`;

function describeError(error: Error): string {
  let stack: unknown;
  let name: unknown;
  let message: unknown;
  try {
    stack = error.stack;
    name = error.name;
    message = error.message;
  } catch {
    return "[Error]";
  }
  const head = `${typeof name === "string" ? name : "Error"}: ${typeof message === "string" ? message : ""}`;
  return cut(
    typeof stack === "string" && stack.includes(String(message))
      ? stack
      : typeof stack === "string"
        ? `${head}\n${stack}`
        : head,
    MAX_STRING * 2,
  );
}

/** What one line has left to spend, and the objects being described. */
interface Walk {
  inside: Set<object>;
  left: number;
}

function describeValue(
  value: unknown,
  depth: number,
  walk: Walk,
  quote: boolean,
): string {
  const shown = describeOne(value, depth, walk, quote);
  walk.left -= shown.length;
  return shown;
}

function describeOne(
  value: unknown,
  depth: number,
  walk: Walk,
  quote: boolean,
): string {
  const inside = walk.inside;
  switch (typeof value) {
    case "string":
      return quote
        ? JSON.stringify(cut(value, MAX_STRING))
        : cut(value, MAX_STRING);
    case "number":
    case "boolean":
    case "undefined":
      return String(value);
    case "bigint":
      return `${value}n`;
    case "symbol":
      return value.toString();
    case "function": {
      const name = dataValue(value, "name");
      return `[Function${typeof name === "string" && name !== "" ? ` ${name}` : ""}]`;
    }
  }
  if (value === null) return "null";
  const object = value as object;
  if (inside.has(object)) return "[Circular]";
  try {
    if (object instanceof Error) return describeError(object);
    if (object instanceof ArrayBuffer)
      return `ArrayBuffer(${call(arrayBufferBytes, object)} bytes)`;
    if (ArrayBuffer.isView(object))
      return object instanceof DataView
        ? `DataView(${call(dataViewBytes, object)} bytes)`
        : `${className(object)}(${call(viewBytes, object)} bytes)`;
    if (typeof Blob !== "undefined" && object instanceof Blob)
      return `${className(object)}(${call(blobSize, object)} bytes)`;
    if (object instanceof Map) return `Map(${call(mapSize, object)})`;
    if (object instanceof Set) return `Set(${call(setSize, object)})`;
    if (object instanceof Promise) return "Promise";
    if (object instanceof WeakMap) return "WeakMap";
    if (object instanceof WeakSet) return "WeakSet";
    if (object instanceof Date) {
      const time = Reflect.apply(dateTime, object, []) as number;
      return Number.isNaN(time)
        ? "Invalid Date"
        : (Reflect.apply(dateIso, object, []) as string);
    }
    if (typeof Node !== "undefined" && object instanceof Node)
      return `<${call(nodeName, object).toLowerCase()}>`;
    if (object instanceof RegExp) return `[RegExp]`;
  } catch {
    return "[Unreadable]";
  }
  const array = Array.isArray(object);
  if (depth >= MAX_DEPTH) return array ? "[Array]" : `[${className(object)}]`;
  inside.add(object);
  try {
    const keys = Reflect.ownKeys(object).filter(
      (key): key is string =>
        typeof key === "string" && !(array && key === "length"),
    );
    const parts: string[] = [];
    for (const key of keys.slice(0, MAX_ENTRIES)) {
      if (walk.left <= 0) {
        parts.push("…");
        break;
      }
      const descriptor = Object.getOwnPropertyDescriptor(object, key);
      if (descriptor === undefined) continue;
      const shown =
        "value" in descriptor
          ? describeValue(descriptor.value, depth + 1, walk, true)
          : descriptor.set === undefined
            ? "[Getter]"
            : "[Getter/Setter]";
      parts.push(
        array && /^\d+$/.test(key)
          ? shown
          : `${/^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key)}: ${shown}`,
      );
    }
    if (keys.length > MAX_ENTRIES && walk.left > 0)
      parts.push(`… ${keys.length - MAX_ENTRIES} more`);
    if (array) return `[${parts.join(", ")}]`;
    const name = className(object);
    return `${name === "Object" ? "" : `${name} `}{${parts.join(", ")}}`;
  } catch {
    return "[Unreadable]";
  } finally {
    inside.delete(object);
  }
}

/** One value, as a console shows it inside a line. */
export function describe(value: unknown): string {
  return cut(
    describeValue(value, 0, { inside: new Set(), left: MAX_LINE }, false),
    MAX_LINE,
  );
}

/** A console call's arguments as one line, within {@link MAX_LINE}. */
export function formatArgs(args: readonly unknown[]): string {
  const walk: Walk = { inside: new Set(), left: MAX_LINE };
  let line = "";
  for (const arg of args.slice(0, MAX_ARGS)) {
    line += (line === "" ? "" : " ") + describeValue(arg, 0, walk, false);
    if (walk.left <= 0 || line.length > MAX_LINE) return cut(line, MAX_LINE);
  }
  if (args.length > MAX_ARGS)
    line += ` … ${args.length - MAX_ARGS} more arguments`;
  return line;
}
