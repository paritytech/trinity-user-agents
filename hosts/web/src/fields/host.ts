import type { PromptRequest } from "../app/prompt.js";
import {
  cryptoRandom,
  concat,
  fromHex,
  toHex,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import {
  FIELDS_PROTOCOL,
  FIELD_SCHEMAS,
  FieldError,
  MAX_FIELDS,
  isFieldId,
  isFieldKind,
  isHex32,
  readReceiveKey,
  type FieldDescriptor,
  type FieldErrorReply,
  type FieldGrant,
  type FieldKind,
  type FieldRequestItem,
  type FieldRole,
  type FieldTheme,
  type OpenedEnvelope,
  type ProviderStatus,
  type ReceiveKey,
} from "../../bridges/fields/protocol.js";
import type { WebImageSurface } from "../../bridges/fields/web.js";
import {
  envelopeId,
  envelopeKeyId,
  openEnvelope,
  sealEnvelope,
  validValue,
  valueLines,
  type SealedField,
} from "./envelope.js";
import { ReceiveKeys } from "./keys.js";
import { fieldSvg, fitsField } from "./svg.js";

/**
 * PROTOTYPE, web-only (`web-fields/2`): the host side of the draft field
 * bridge. It keeps provider declarations and values, asks the user before
 * any value leaves for a product, seals values to a recipient's receiving
 * key, makes and keeps this wallet's receiving keys, opens envelopes into
 * handles and draws handles as images in products. A product gets
 * descriptors and handles, never a value. Delivery is the product's.
 */

/** Signs and checks receiving key bindings in the core (signing hosts only). */
export interface KeyBinder {
  sign(
    product: string,
    index: Uint8Array,
    binding: Uint8Array,
  ): Promise<{ account: Uint8Array; signature: Uint8Array }>;
  verify(
    product: string,
    account: Uint8Array,
    binding: Uint8Array,
    signature: Uint8Array,
  ): Promise<boolean>;
}

/** The signed-in wallet the bridge works for. */
export interface FieldSession {
  /** The host's network id, which scopes stored keys. */
  network: string;
  /** The network's People chain genesis hash, `0x` hex, which a binding names. */
  people: string;
  walletId: string;
  entropy: Uint8Array;
}

/** A label the host prints: the product's public text, then protected values. */
export interface FieldLabel {
  product: string;
  title: string;
  lines: string[];
  fields: { kind: FieldKind; lines: string[] }[];
}

export interface FieldHostOptions {
  ask<T>(request: PromptRequest<T>): Promise<T>;
  binder: KeyBinder;
  /** Where receiving keys are kept, encrypted. */
  storage: Pick<Storage, "getItem" | "setItem">;
  /**
   * The local image service: serves `svg` once to `product` and answers its
   * URL, when it stops serving it (milliseconds since the epoch), and how to
   * end it unused. Ending is best effort: a revoke that fails, or a tab
   * closed before it is sent, leaves the URL until it expires.
   */
  images: {
    issue(image: {
      product: string;
      svg: string;
    }): Promise<{ url: string; expires: number; revoke(): Promise<void> }>;
  };
  /**
   * Print `label` from a host document. Products cannot print: their frame
   * has no `allow-modals`. `signal` aborts it when the product or session ends.
   */
  print(label: FieldLabel, signal: AbortSignal): Promise<void>;
  now?(): number;
  random?: RandomBytes;
}

interface Provider {
  /** The name the provider gives itself. Shown beside its verified product id, never alone. */
  name: string;
  fields: FieldKind[];
  values: Partial<Record<FieldKind, string>>;
}

/** A value a handle stands for, on one connection. */
interface Held {
  kind: FieldKind;
  lines: string[];
  /**
   * This value's images the service may still serve, each by how to end it
   * and when it expires. An image already drawn stays drawn.
   */
  images: Map<() => Promise<void>, number>;
}

/** The most handles one connection holds at once. */
const MAX_HANDLES = 64;
/** The most images of one handle the service may serve at once. */
const MAX_IMAGES = 50;
const MAX_LABEL_LINES = 6;
const MAX_LABEL_CHARS = 80;

const KIND_NAMES: Record<FieldKind, string> = {
  address: "address",
  phone: "phone",
};

/** Printable text, trimmed, 1 to `max` characters, or null. */
const shortText = (value: unknown, max: number): string | null => {
  if (typeof value !== "string") return null;
  const text = value.replace(/[\p{C}]/gu, "").trim();
  return text.length > 0 && text.length <= max ? text : null;
};

/**
 * A failure as the product sees it. Only {@link FieldError}s keep their text;
 * anything else is reported generically, so no unexpected message crosses.
 */
const errorReply = (error: unknown): FieldErrorReply =>
  error instanceof FieldError
    ? { code: error.code, message: error.message }
    : { code: "unavailable", message: "Request failed." };

const shortAccount = (account: string) =>
  `${account.slice(0, 8)}…${account.slice(-6)}`;

/** The 80 bytes a binding signs besides product and account. */
const bindingBytes = (key: {
  keyId: string;
  publicKey: string;
  network: string;
}): Uint8Array =>
  concat(fromHex(key.keyId), fromHex(key.publicKey), fromHex(key.network));

/** Read a product's field request list. Throws `invalid`. */
function readItems(value: unknown): FieldRequestItem[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_FIELDS)
    throw new FieldError("invalid", `Ask for 1 to ${MAX_FIELDS} fields.`);
  const items = value.map((item: unknown): FieldRequestItem => {
    const field = (item ?? {}) as Record<string, unknown>;
    if (
      !isFieldId(field.id) ||
      !isFieldKind(field.kind) ||
      (field.optional !== undefined && typeof field.optional !== "boolean")
    )
      throw new FieldError("invalid", "A requested field is malformed.");
    return {
      id: field.id,
      kind: field.kind,
      optional: field.optional === true,
    };
  });
  if (
    new Set(items.map((item) => item.id)).size !== items.length ||
    new Set(items.map((item) => item.kind)).size !== items.length
  )
    throw new FieldError(
      "invalid",
      "Ask for each field id and each kind once.",
    );
  if (items.every((item) => item.optional))
    throw new FieldError("invalid", "Ask for at least one required field.");
  return items;
}

export class FieldHost {
  private session: FieldSession | null = null;
  private readonly keys: ReceiveKeys;
  private readonly providers = new Map<string, Provider>();
  private readonly connections = new Set<() => void>();
  private printing = false;

  constructor(private readonly options: FieldHostOptions) {
    this.keys = new ReceiveKeys(options.storage, this.random, this.now);
  }

  private readonly now = (): number => this.options.now?.() ?? Date.now();
  private readonly random: RandomBytes = (length) =>
    (this.options.random ?? cryptoRandom)(length);

  /**
   * Work for `session`, or for nobody. A change ends every connection, drops
   * provider values and handles, ends images not loaded yet, and locks
   * receiving keys; stored
   * keys stay and open again for the same wallet.
   */
  setSession(session: FieldSession | null): void {
    const same =
      session !== null &&
      this.session !== null &&
      session.network === this.session.network &&
      session.walletId === this.session.walletId;
    if (same) return;
    for (const close of [...this.connections]) close();
    this.providers.clear();
    this.session = session;
    if (session === null) this.keys.lock();
    else this.keys.unlock(session);
  }

  /** Attach an already source- and origin-checked product port, with the role the host gave it. */
  connect(product: string, role: FieldRole, port: MessagePort): () => void {
    const session = this.session;
    const controller = new AbortController();
    const heldValues = new Map<string, Held>();
    let queue = Promise.resolve();
    const active = (): FieldSession => {
      if (
        session === null ||
        session !== this.session ||
        controller.signal.aborted
      )
        throw new FieldError(
          "session",
          "Session ended. Sign in and reopen the app.",
        );
      return session;
    };
    const blank = async (held: Held): Promise<void> => {
      const revokes = [...held.images.keys()].map((revoke) => revoke());
      held.images.clear();
      await Promise.all(revokes);
    };
    const close = () => {
      if (controller.signal.aborted) return;
      controller.abort();
      port.close();
      for (const held of heldValues.values()) void blank(held);
      heldValues.clear();
      this.connections.delete(close);
    };
    this.connections.add(close);
    const ask = async <T>(request: PromptRequest<T>): Promise<T> => {
      active();
      const result = await this.options.ask({
        ...request,
        from: product,
        signal: controller.signal,
      });
      active();
      return result;
    };
    const hold = (fields: readonly SealedField[]): FieldDescriptor[] => {
      if (heldValues.size + fields.length > MAX_HANDLES)
        throw new FieldError(
          "limit",
          "Too many fields held. Release some first.",
        );
      return fields.map((field) => {
        const handle = toHex(this.random(16)).slice(2);
        heldValues.set(handle, {
          kind: field.kind,
          lines: valueLines(field.kind, field.value),
          images: new Map(),
        });
        return {
          id: field.id,
          kind: field.kind,
          schema: field.schema,
          handle,
        };
      });
    };
    const held = (handle: unknown): Held => {
      const found =
        typeof handle === "string" ? heldValues.get(handle) : undefined;
      if (found === undefined)
        throw new FieldError(
          "unavailable",
          "This field is no longer held. Open it again.",
        );
      return found;
    };

    const provider = async (
      request: Record<string, unknown>,
    ): Promise<unknown> => {
      if (request.op === "declare") {
        const fields = request.fields;
        if (
          !Array.isArray(fields) ||
          fields.length === 0 ||
          fields.length > MAX_FIELDS ||
          !fields.every(isFieldKind) ||
          new Set(fields).size !== fields.length
        )
          throw new FieldError("invalid", "Declare known field kinds once.");
        const name = shortText(request.name, 40);
        if (name === null)
          throw new FieldError(
            "invalid",
            "Name the app in 1 to 40 characters.",
          );
        // No sheet: only apps the host made providers reach this, declaring
        // discloses and spends nothing, and every disclosure later asks.
        const known = this.providers.get(product);
        if (known !== undefined) return { registered: known.fields };
        this.providers.set(product, {
          name,
          fields: fields as FieldKind[],
          values: {},
        });
        return { registered: fields };
      }
      if (request.op === "save") {
        const found = this.providers.get(product);
        if (found === undefined)
          throw new FieldError("invalid", "Declare fields first.");
        if (typeof request.values !== "object" || request.values === null)
          throw new FieldError("invalid", "Invalid values.");
        const next: Partial<Record<FieldKind, string>> = {};
        for (const [kind, value] of Object.entries(request.values)) {
          if (value === "" || value === undefined) continue;
          if (
            !found.fields.includes(kind as FieldKind) ||
            !validValue(kind as FieldKind, value)
          )
            throw new FieldError(
              "invalid",
              "Use declared fields, each 1 to 160 characters, an address in at most 6 lines.",
            );
          if (
            !fitsField(kind as FieldKind, valueLines(kind as FieldKind, value))
          )
            throw new FieldError(
              "invalid",
              `This ${KIND_NAMES[kind as FieldKind]} is too long to show. Shorten it.`,
            );
          next[kind as FieldKind] = value;
        }
        found.values = next;
        return { saved: Object.keys(next) };
      }
      if (request.op === "status") {
        const found = this.providers.get(product);
        const status: ProviderStatus = {
          registered: found !== undefined,
          name: found?.name ?? null,
          fields: found?.fields ?? [],
          saved:
            found === undefined
              ? []
              : found.fields.filter((kind) => found.values[kind] !== undefined),
        };
        return status;
      }
      throw new FieldError("unsupported", "Provider operation not supported.");
    };

    const createReceiveKey = async (
      request: Record<string, unknown>,
    ): Promise<ReceiveKey> => {
      const current = active();
      if (!isHex32(request.index))
        throw new FieldError("invalid", "An account index is 32 bytes of hex.");
      const record = this.keys.ensure(product);
      const unsigned = {
        keyId: record.keyId,
        publicKey: record.publicKey,
        network: current.people.toLowerCase(),
      };
      // No sheet: the key is new or this app's own, the signature is under
      // a context only bindings use, and nothing is disclosed or spent.
      const signed = await this.options.binder.sign(
        product,
        fromHex(request.index),
        bindingBytes(unsigned),
      );
      active();
      const account = toHex(signed.account);
      this.keys.bound(product, account);
      return {
        v: 1,
        ...unsigned,
        product,
        account,
        binding: { kind: "host-signature", signature: toHex(signed.signature) },
      };
    };

    const request = async (
      input: Record<string, unknown>,
    ): Promise<FieldGrant> => {
      const current = active();
      const items = readItems(input.fields);
      const purpose = shortText(input.purpose, 80);
      if (purpose === null)
        throw new FieldError(
          "invalid",
          "Give a purpose of 1 to 80 characters.",
        );
      if (!isHex32(input.context))
        throw new FieldError("invalid", "A context is 32 bytes of hex.");
      const context = input.context;
      let recipient: ReceiveKey | null = null;
      if (input.recipient !== undefined) {
        recipient = readReceiveKey(input.recipient);
        if (recipient === null)
          throw new FieldError("invalid", "The recipient key is malformed.");
        if (recipient.product !== product)
          throw new FieldError(
            "invalid",
            "The recipient key is for another app.",
          );
        if (recipient.network !== current.people.toLowerCase())
          throw new FieldError(
            "invalid",
            "The recipient key is for another network.",
          );
        const valid = await this.options.binder.verify(
          product,
          fromHex(recipient.account),
          bindingBytes(recipient),
          fromHex(recipient.binding.signature),
        );
        active();
        if (!valid)
          throw new FieldError(
            "invalid",
            "The recipient key's host signature does not verify.",
          );
      }
      // Values are read once, here: what the sheet shows is what is sealed,
      // even if a provider saves new values while it is open.
      const required = items.filter((item) => !item.optional);
      const optional = items.filter((item) => item.optional);
      const options = [...this.providers].flatMap(([id, found]) => {
        if (required.some((item) => found.values[item.kind] === undefined))
          return [];
        const extra = optional.filter(
          (item) => found.values[item.kind] !== undefined,
        );
        const pick = (chosen: FieldRequestItem[]) =>
          chosen.map((item) => ({
            id: item.id,
            kind: item.kind,
            schema: FIELD_SCHEMAS[item.kind],
            value: found.values[item.kind]!,
          }));
        const from = `${found.name} · ${id}`;
        return [
          ...(extra.length > 0
            ? [{ from, fields: pick([...required, ...extra]) }]
            : []),
          { from, fields: pick(required) },
        ];
      });
      const wanted = required
        .map((item) => KIND_NAMES[item.kind])
        .join(" and ");
      if (options.length === 0)
        throw new FieldError(
          "unavailable",
          `No details app holds your ${wanted} yet. Save it in one first, such as Field notebook.`,
        );
      const describe = (fields: readonly SealedField[]) =>
        fields
          .map(
            (field) =>
              `${KIND_NAMES[field.kind]}: ${valueLines(field.kind, field.value).join(", ")}`,
          )
          .join(" · ");
      const chosen = await ask<number | null>({
        title:
          recipient === null
            ? `Show your ${wanted} in this app?`
            : `Send your ${wanted}?`,
        subtitle: `The app says: “${purpose}”`,
        fields: [
          {
            label: "Goes to",
            value:
              recipient === null
                ? "Nobody. Your host draws it in this app; the app's scripts cannot read it."
                : `Account ${shortAccount(recipient.account)} in ${product}. Its host made the receiving key and signed for it.`,
          },
        ],
        details: {
          summary: "Who can read it",
          fields: [
            {
              label: "Readers",
              value:
                recipient === null
                  ? "You, your host and the details app it comes from. Not the app's scripts."
                  : "The person who holds that account, through their host, can read and copy these values. Your host and the details app know them too. The app's scripts, on either side, get none.",
            },
            ...(recipient === null
              ? []
              : [
                  {
                    label: "Recipient check",
                    value:
                      "Their host signed this key as that account, with a signature apps cannot make. It does not prove who the person is, or that their host is honest.",
                  },
                ]),
            {
              label: "Limits",
              value:
                "Web prototype, not a sandbox or a production key store. No revocation.",
            },
          ],
        },
        layout: "list",
        choices: [
          ...options.map((option, index) => ({
            label: `Share ${describe(option.fields)} · from ${option.from}`,
            value: index,
          })),
          { label: "Cancel", value: null, tone: "cancel" as const },
        ],
        dismissed: null,
      });
      if (chosen === null)
        throw new FieldError("cancelled", "You chose not to share.");
      const fields = options[chosen]!.fields;
      const envelope =
        recipient === null
          ? undefined
          : toHex(
              sealEnvelope(
                recipient,
                { product, account: recipient.account, context },
                fields,
                this.random,
              ),
            );
      return {
        fields: hold(fields),
        ...(envelope === undefined ? {} : { envelope }),
      };
    };

    const open = (input: Record<string, unknown>): OpenedEnvelope => {
      active();
      if (!isHex32(input.context))
        throw new FieldError("invalid", "A context is 32 bytes of hex.");
      let bytes: Uint8Array;
      try {
        bytes = fromHex(input.envelope as string);
      } catch {
        throw new FieldError("invalid", "An envelope is hex.");
      }
      const key = this.keys.open(product, envelopeKeyId(bytes));
      if (key === null)
        throw new FieldError(
          "invalid",
          "This envelope is not sealed to this app's receiving key.",
        );
      try {
        const fields = openEnvelope(key.pair, bytes, {
          product,
          accounts: key.accounts,
          context: input.context,
        });
        return { envelopeId: envelopeId(bytes), fields: hold(fields) };
      } finally {
        key.pair.secret.fill(0);
      }
    };

    /** Answers once the released values' unused images no longer load. */
    const release = async (input: Record<string, unknown>) => {
      const handles = Array.isArray(input.handles) ? input.handles : [];
      const revoking: Promise<void>[] = [];
      let released = 0;
      for (const handle of handles) {
        const found =
          typeof handle === "string" ? heldValues.get(handle) : undefined;
        if (found === undefined) continue;
        revoking.push(blank(found));
        heldValues.delete(handle as string);
        released += 1;
      }
      await Promise.all(revoking);
      return { released };
    };

    const display = async (
      input: Record<string, unknown>,
    ): Promise<{ surface: WebImageSurface }> => {
      const value = held(input.handle);
      const theme: FieldTheme = input.theme === "dark" ? "dark" : "paper";
      const now = this.now();
      for (const [revoke, expires] of value.images)
        if (expires <= now) value.images.delete(revoke);
      if (value.images.size >= MAX_IMAGES)
        throw new FieldError("limit", "Too many views. Open it again.");
      const svg = fieldSvg(value.kind, value.lines, theme);
      let image: Awaited<ReturnType<FieldHostOptions["images"]["issue"]>>;
      try {
        image = await this.options.images.issue({ product, svg });
      } catch {
        active();
        throw new FieldError(
          "unavailable",
          "The host could not draw this field. Try again.",
        );
      }
      // The handle may have been released, or the session ended, meanwhile.
      if (
        controller.signal.aborted ||
        heldValues.get(input.handle as string) !== value
      ) {
        void image.revoke();
        active();
        throw new FieldError(
          "unavailable",
          "This field is no longer held. Open it again.",
        );
      }
      value.images.set(image.revoke, image.expires);
      return { surface: { platform: "web-image", url: image.url } };
    };

    const print = async (input: Record<string, unknown>) => {
      const title = shortText(input.title, MAX_LABEL_CHARS);
      const lines = Array.isArray(input.lines) ? input.lines : [];
      const handles = Array.isArray(input.handles) ? input.handles : [];
      if (
        title === null ||
        lines.length > MAX_LABEL_LINES ||
        lines.some((line) => shortText(line, MAX_LABEL_CHARS) === null) ||
        handles.length === 0 ||
        handles.length > MAX_FIELDS
      )
        throw new FieldError(
          "invalid",
          "Print a title, up to 6 short lines and 1 to 4 fields.",
        );
      const fields = handles.map((handle) => {
        const found = held(handle);
        return { kind: found.kind, lines: [...found.lines] };
      });
      if (this.printing)
        throw new FieldError("busy", "A label is already printing.");
      this.printing = true;
      try {
        await this.options.print(
          {
            product,
            title,
            lines: lines.map((line) => shortText(line, MAX_LABEL_CHARS)!),
            fields,
          },
          controller.signal,
        );
      } finally {
        this.printing = false;
      }
      active();
      return { printed: true };
    };

    const call = async (input: Record<string, unknown>): Promise<unknown> => {
      active();
      if (role === "provider") return provider(input);
      switch (input.op) {
        case "createReceiveKey":
          return createReceiveKey(input);
        case "request":
          return request(input);
        case "open":
          return open(input);
        case "release":
          return release(input);
        case "display":
          return display(input);
        case "print":
          return print(input);
      }
      throw new FieldError("unsupported", "Operation not supported.");
    };
    port.onmessage = (event: MessageEvent<unknown>) => {
      const input = event.data;
      if (typeof input !== "object" || input === null) return;
      const record = input as Record<string, unknown>;
      if (typeof record.id !== "number" || !Number.isSafeInteger(record.id))
        return;
      const id = record.id;
      queue = queue.then(async () => {
        try {
          const result = await call(record);
          active();
          port.postMessage({ id, result });
        } catch (error) {
          if (!controller.signal.aborted)
            port.postMessage({ id, error: errorReply(error) });
        }
      });
    };
    port.start();
    port.postMessage({ protocol: FIELDS_PROTOCOL, role });
    return close;
  }
}
