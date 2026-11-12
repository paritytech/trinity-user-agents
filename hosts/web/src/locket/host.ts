import type { PromptRequest } from "../app/prompt.js";
import {
  cryptoRandom,
  concat,
  fromHex,
  toHex,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import {
  LOCKET_PROTOCOL,
  DETAIL_SCHEMAS,
  LocketError,
  MAX_DETAILS,
  isDetailId,
  isDetailKind,
  isHex32,
  readRecipientKey,
  type DetailDescriptor,
  type LocketErrorReply,
  type DetailGrant,
  type DetailKind,
  type DetailRequestItem,
  type LocketRole,
  type DetailTheme,
  type OpenedEnvelope,
  type ProviderStatus,
  type RecipientKey,
} from "../../bridges/locket/protocol.js";
import type { WebImageSurface } from "../../bridges/locket/web.js";
import {
  envelopeId,
  envelopeKeyId,
  openEnvelope,
  sealEnvelope,
  validValue,
  valueLines,
  type SealedDetail,
} from "./envelope.js";
import { RecipientKeys } from "./keys.js";
import { detailSvg, fitsDetail } from "./svg.js";

/**
 * PROTOTYPE, web-only (`web-locket/1`): the host side of the draft Locket
 * bridge. It keeps provider declarations and values, asks the user before
 * any value leaves for a product, seals values to a recipient's key, makes
 * and keeps this wallet's recipient keys, opens envelopes into refs and
 * draws refs as images in products. A product gets descriptors and refs,
 * never a value. Delivery is the product's.
 */

/** Signs and checks recipient key bindings in the core (signing hosts only). */
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
export interface LocketSession {
  /** The host's network id, which scopes stored keys. */
  network: string;
  /** The network's People chain genesis hash, `0x` hex, which a binding names. */
  people: string;
  walletId: string;
  entropy: Uint8Array;
}

/** A label the host prints: the product's public text, then protected values. */
export interface LocketLabel {
  product: string;
  title: string;
  lines: string[];
  details: { kind: DetailKind; lines: string[] }[];
}

export interface LocketHostOptions {
  ask<T>(request: PromptRequest<T>): Promise<T>;
  binder: KeyBinder;
  /** Where recipient keys are kept, encrypted. */
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
  print(label: LocketLabel, signal: AbortSignal): Promise<void>;
  now?(): number;
  random?: RandomBytes;
}

interface Provider {
  /** The name the provider gives itself. Shown beside its verified product id, never alone. */
  name: string;
  details: DetailKind[];
  values: Partial<Record<DetailKind, string>>;
}

/** A value a ref stands for, on one connection. */
interface Held {
  kind: DetailKind;
  lines: string[];
  /**
   * This value's images the service may still serve, each by how to end it
   * and when it expires. An image already drawn stays drawn.
   */
  images: Map<() => Promise<void>, number>;
  /** Images being issued now: they count against the limit before they exist. */
  issuing: number;
}

/** The most refs one connection holds at once. */
const MAX_REFS = 64;
/**
 * The most calls one connection has unanswered at once. More fail `busy`.
 * A cancel never counts, and a `release` is never refused, so cleanup always
 * goes through.
 */
const MAX_PENDING = 32;
/** The most images of one ref the service may serve at once. */
const MAX_IMAGES = 50;
const MAX_LABEL_LINES = 6;
const MAX_LABEL_CHARS = 80;

const KIND_NAMES: Record<DetailKind, string> = {
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
 * A failure as the product sees it. Only {@link LocketError}s keep their text;
 * anything else is reported generically, so no unexpected message crosses.
 */
const errorReply = (error: unknown): LocketErrorReply =>
  error instanceof LocketError
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

/** Read a product's detail request list. Throws `invalid`. */
function readItems(value: unknown): DetailRequestItem[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_DETAILS)
    throw new LocketError("invalid", `Ask for 1 to ${MAX_DETAILS} details.`);
  const items = value.map((item: unknown): DetailRequestItem => {
    const detail = (item ?? {}) as Record<string, unknown>;
    if (
      !isDetailId(detail.id) ||
      !isDetailKind(detail.kind) ||
      (detail.optional !== undefined && typeof detail.optional !== "boolean")
    )
      throw new LocketError("invalid", "A requested detail is malformed.");
    return {
      id: detail.id,
      kind: detail.kind,
      optional: detail.optional === true,
    };
  });
  if (
    new Set(items.map((item) => item.id)).size !== items.length ||
    new Set(items.map((item) => item.kind)).size !== items.length
  )
    throw new LocketError(
      "invalid",
      "Ask for each detail id and each kind once.",
    );
  if (items.every((item) => item.optional))
    throw new LocketError("invalid", "Ask for at least one required detail.");
  return items;
}

export class LocketHost {
  private session: LocketSession | null = null;
  private readonly keys: RecipientKeys;
  private readonly providers = new Map<string, Provider>();
  private readonly connections = new Set<() => void>();
  private printing = false;

  constructor(private readonly options: LocketHostOptions) {
    this.keys = new RecipientKeys(options.storage, this.random, this.now);
  }

  private readonly now = (): number => this.options.now?.() ?? Date.now();
  private readonly random: RandomBytes = (length) =>
    (this.options.random ?? cryptoRandom)(length);

  /**
   * Work for `session`, or for nobody. A change ends every connection, drops
   * provider values and refs, ends images not loaded yet, and locks
   * recipient keys; stored
   * keys stay and open again for the same wallet.
   */
  setSession(session: LocketSession | null): void {
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
  connect(product: string, role: LocketRole, port: MessagePort): () => void {
    const session = this.session;
    const controller = new AbortController();
    const heldValues = new Map<string, Held>();
    /** Requests, one at a time: each may show the user a sheet. */
    let requests = Promise.resolve();
    /** Calls not answered yet, by id, so the product can withdraw one. */
    const pending = new Map<number, AbortController>();
    /**
     * The session, while the connection lasts and `signal`'s call stands.
     * Every operation checks it after each wait, so a withdrawn call never
     * holds, seals or keeps anything after it was withdrawn.
     */
    const active = (signal: AbortSignal = controller.signal): LocketSession => {
      if (
        session === null ||
        session !== this.session ||
        controller.signal.aborted
      )
        throw new LocketError(
          "session",
          "Session ended. Sign in and reopen the app.",
        );
      if (signal.aborted)
        throw new LocketError("timeout", "The app withdrew this request.");
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
    const ask = async <T>(
      request: PromptRequest<T>,
      signal: AbortSignal,
    ): Promise<T> => {
      active(signal);
      // Aborting closes the sheet, or never shows it.
      const result = await this.options.ask({
        ...request,
        from: product,
        signal,
      });
      active(signal);
      return result;
    };
    const hold = (details: readonly SealedDetail[]): DetailDescriptor[] => {
      if (heldValues.size + details.length > MAX_REFS)
        throw new LocketError(
          "limit",
          "Too many details held. Release some first.",
        );
      return details.map((detail) => {
        const ref = toHex(this.random(16)).slice(2);
        heldValues.set(ref, {
          kind: detail.kind,
          lines: valueLines(detail.kind, detail.value),
          images: new Map(),
          issuing: 0,
        });
        return {
          id: detail.id,
          kind: detail.kind,
          schema: detail.schema,
          ref,
        };
      });
    };
    const held = (ref: unknown): Held => {
      const found = typeof ref === "string" ? heldValues.get(ref) : undefined;
      if (found === undefined)
        throw new LocketError(
          "missing-ref",
          "This detail is no longer held. Open it again.",
        );
      return found;
    };

    const provider = async (
      request: Record<string, unknown>,
    ): Promise<unknown> => {
      if (request.op === "declare") {
        const details = request.details;
        if (
          !Array.isArray(details) ||
          details.length === 0 ||
          details.length > MAX_DETAILS ||
          !details.every(isDetailKind) ||
          new Set(details).size !== details.length
        )
          throw new LocketError("invalid", "Declare known detail kinds once.");
        const name = shortText(request.name, 40);
        if (name === null)
          throw new LocketError(
            "invalid",
            "Name the app in 1 to 40 characters.",
          );
        // No sheet: only apps the host made providers reach this, declaring
        // discloses and spends nothing, and every disclosure later asks.
        const known = this.providers.get(product);
        if (known !== undefined) {
          if (
            known.name !== name ||
            known.details.length !== details.length ||
            !known.details.every((kind) => details.includes(kind))
          )
            throw new LocketError(
              "invalid",
              "This app already declared another name or other details.",
            );
          return { registered: known.details };
        }
        this.providers.set(product, {
          name,
          details: details as DetailKind[],
          values: {},
        });
        return { registered: details };
      }
      if (request.op === "setValues") {
        const found = this.providers.get(product);
        if (found === undefined)
          throw new LocketError("invalid", "Declare details first.");
        if (typeof request.values !== "object" || request.values === null)
          throw new LocketError("invalid", "Invalid values.");
        // A full replacement: every declared kind, a value or null, and no other.
        const values = request.values as Record<string, unknown>;
        if (
          Object.keys(values).length !== found.details.length ||
          !found.details.every((kind) => Object.hasOwn(values, kind))
        )
          throw new LocketError(
            "invalid",
            "Give every declared detail, null for none, and no other.",
          );
        const next: Partial<Record<DetailKind, string>> = {};
        for (const kind of found.details) {
          const value = values[kind];
          if (value === null) continue;
          if (!validValue(kind, value))
            throw new LocketError(
              "invalid",
              "Use 1 to 160 characters for each detail, an address in at most 6 lines.",
            );
          if (!fitsDetail(kind, valueLines(kind, value)))
            throw new LocketError(
              "invalid",
              `This ${KIND_NAMES[kind]} is too long to show. Shorten it.`,
            );
          next[kind] = value;
        }
        found.values = next;
        return { saved: Object.keys(next) };
      }
      if (request.op === "status") {
        const found = this.providers.get(product);
        const status: ProviderStatus = {
          registered: found !== undefined,
          name: found?.name ?? null,
          details: found?.details ?? [],
          saved:
            found === undefined
              ? []
              : found.details.filter(
                  (kind) => found.values[kind] !== undefined,
                ),
        };
        return status;
      }
      throw new LocketError("unsupported", "Provider operation not supported.");
    };

    const recipientKey = async (
      request: Record<string, unknown>,
      signal: AbortSignal,
    ): Promise<RecipientKey> => {
      const current = active(signal);
      if (!isHex32(request.index))
        throw new LocketError(
          "invalid",
          "An account index is 32 bytes of hex.",
        );
      const index = request.index;
      const network = current.people.toLowerCase();
      // No sheet: the key is new or this account's own, the signature is
      // under a context only bindings use, and nothing is disclosed or spent.
      // The core resolves the account from the index; the caller names none.
      const bind = async (key: { keyId: string; publicKey: string }) => {
        const signed = await this.options.binder.sign(
          product,
          fromHex(index),
          bindingBytes({ ...key, network }),
        );
        active(signal);
        const answer: RecipientKey = {
          v: 1,
          keyId: key.keyId,
          publicKey: key.publicKey,
          network,
          product,
          account: toHex(signed.account),
          binding: {
            kind: "host-signature",
            signature: toHex(signed.signature),
          },
        };
        return answer;
      };
      const rebind = async (kept: {
        keyId: string;
        publicKey: string;
        account: string;
      }) => {
        const key = await bind(kept);
        if (key.account !== kept.account)
          throw new LocketError(
            "invalid",
            "The stored recipient key is for another account.",
          );
        return key;
      };
      const kept = this.keys.find(product, index);
      if (kept !== null) return rebind(kept);
      const fresh = this.keys.prepare();
      let key: RecipientKey;
      try {
        key = await bind(fresh);
      } catch (error) {
        fresh.secret.fill(0);
        throw error;
      }
      try {
        this.keys.keep(product, index, key.account, fresh);
        return key;
      } catch (error) {
        // Another call stored this account's key meanwhile: answer that one.
        const stored = this.keys.find(product, index);
        if (error instanceof LocketError && error.code === "busy" && stored)
          return rebind(stored);
        throw error;
      }
    };

    const request = async (
      input: Record<string, unknown>,
      signal: AbortSignal,
    ): Promise<DetailGrant> => {
      const current = active(signal);
      const items = readItems(input.details);
      const purpose = shortText(input.purpose, 80);
      if (purpose === null)
        throw new LocketError(
          "invalid",
          "Give a purpose of 1 to 80 characters.",
        );
      if (!isHex32(input.context))
        throw new LocketError("invalid", "A context is 32 bytes of hex.");
      const context = input.context;
      let recipient: RecipientKey | null = null;
      if (input.recipient !== undefined) {
        recipient = readRecipientKey(input.recipient);
        if (recipient === null)
          throw new LocketError("invalid", "The recipient key is malformed.");
        if (recipient.product !== product)
          throw new LocketError(
            "invalid",
            "The recipient key is for another app.",
          );
        if (recipient.network !== current.people.toLowerCase())
          throw new LocketError(
            "invalid",
            "The recipient key is for another network.",
          );
        const valid = await this.options.binder.verify(
          product,
          fromHex(recipient.account),
          bindingBytes(recipient),
          fromHex(recipient.binding.signature),
        );
        active(signal);
        if (!valid)
          throw new LocketError(
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
        const pick = (chosen: DetailRequestItem[]) =>
          chosen.map((item) => ({
            id: item.id,
            kind: item.kind,
            schema: DETAIL_SCHEMAS[item.kind],
            value: found.values[item.kind]!,
          }));
        const from = `${found.name} · ${id}`;
        return [
          ...(extra.length > 0
            ? [{ from, details: pick([...required, ...extra]) }]
            : []),
          { from, details: pick(required) },
        ];
      });
      const wanted = required
        .map((item) => KIND_NAMES[item.kind])
        .join(" and ");
      if (options.length === 0)
        throw new LocketError(
          "unavailable",
          `No details app holds your ${wanted} yet. Save it in one first.`,
        );
      const describe = (details: readonly SealedDetail[]) =>
        details
          .map(
            (detail) =>
              `${KIND_NAMES[detail.kind]}: ${valueLines(detail.kind, detail.value).join(", ")}`,
          )
          .join(" · ");
      const chosen = await ask<number | null>(
        {
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
              label: `Share ${describe(option.details)} · from ${option.from}`,
              value: index,
            })),
            { label: "Cancel", value: null, tone: "cancel" as const },
          ],
          dismissed: null,
        },
        signal,
      );
      if (chosen === null)
        throw new LocketError("cancelled", "You chose not to share.");
      const details = options[chosen]!.details;
      const envelope =
        recipient === null
          ? undefined
          : toHex(
              sealEnvelope(
                recipient,
                { product, account: recipient.account, context },
                details,
                this.random,
              ),
            );
      return {
        details: hold(details),
        ...(envelope === undefined ? {} : { envelope }),
      };
    };

    const open = (input: Record<string, unknown>): OpenedEnvelope => {
      active();
      if (!isHex32(input.context))
        throw new LocketError("invalid", "A context is 32 bytes of hex.");
      let bytes: Uint8Array;
      try {
        bytes = fromHex(input.envelope as string);
      } catch {
        throw new LocketError("invalid", "An envelope is hex.");
      }
      const key = this.keys.open(product, envelopeKeyId(bytes));
      if (key === null)
        throw new LocketError(
          "missing-key",
          "No recipient key of this app on this device matches this envelope, so it cannot be opened here. If it was meant for this account, ask the sender to send it again.",
        );
      try {
        // A legacy shared key may name several accounts: the envelope opens
        // once, for the one it names, which the answer reports.
        const opened = openEnvelope(key.pair, bytes, {
          product,
          accounts: key.accounts,
          context: input.context,
        });
        return {
          envelopeId: envelopeId(bytes),
          account: opened.account,
          details: hold(opened.details),
        };
      } finally {
        key.pair.secret.fill(0);
      }
    };

    /** Answers once the released values' unused images no longer load. */
    const release = async (input: Record<string, unknown>) => {
      const refs = Array.isArray(input.refs) ? input.refs : [];
      const revoking: Promise<void>[] = [];
      let released = 0;
      for (const ref of refs) {
        const found = typeof ref === "string" ? heldValues.get(ref) : undefined;
        if (found === undefined) continue;
        revoking.push(blank(found));
        heldValues.delete(ref as string);
        released += 1;
      }
      await Promise.all(revoking);
      return { released };
    };

    const display = async (
      input: Record<string, unknown>,
      signal: AbortSignal,
    ): Promise<{ surface: WebImageSurface }> => {
      const value = held(input.ref);
      const theme: DetailTheme = input.theme === "dark" ? "dark" : "paper";
      const now = this.now();
      for (const [revoke, expires] of value.images)
        if (expires <= now) value.images.delete(revoke);
      if (value.images.size + value.issuing >= MAX_IMAGES)
        throw new LocketError("limit", "Too many views. Open it again.");
      const svg = detailSvg(value.kind, value.lines, theme);
      // The slot is taken before the wait, so concurrent displays cannot
      // all pass the check above.
      value.issuing += 1;
      let image: Awaited<ReturnType<LocketHostOptions["images"]["issue"]>>;
      try {
        image = await this.options.images.issue({ product, svg });
      } catch {
        active(signal);
        throw new LocketError(
          "unavailable",
          "The host could not draw this detail. Try again.",
        );
      } finally {
        value.issuing -= 1;
      }
      // The ref may have been released, the call withdrawn or the
      // session ended meanwhile.
      if (signal.aborted || heldValues.get(input.ref as string) !== value) {
        void image.revoke();
        active(signal);
        throw new LocketError(
          "missing-ref",
          "This detail is no longer held. Open it again.",
        );
      }
      value.images.set(image.revoke, image.expires);
      return { surface: { platform: "web-image", url: image.url } };
    };

    const print = async (
      input: Record<string, unknown>,
      signal: AbortSignal,
    ) => {
      const title = shortText(input.title, MAX_LABEL_CHARS);
      const lines = Array.isArray(input.lines) ? input.lines : [];
      const refs = Array.isArray(input.refs) ? input.refs : [];
      if (
        title === null ||
        lines.length > MAX_LABEL_LINES ||
        lines.some((line) => shortText(line, MAX_LABEL_CHARS) === null) ||
        refs.length === 0 ||
        refs.length > MAX_DETAILS
      )
        throw new LocketError(
          "invalid",
          "Print a title, up to 6 short lines and 1 to 4 details.",
        );
      const details = refs.map((ref) => {
        const found = held(ref);
        return { kind: found.kind, lines: [...found.lines] };
      });
      if (this.printing)
        throw new LocketError("busy", "A label is already printing.");
      this.printing = true;
      try {
        await this.options.print(
          {
            product,
            title,
            lines: lines.map((line) => shortText(line, MAX_LABEL_CHARS)!),
            details,
          },
          signal,
        );
      } finally {
        this.printing = false;
      }
      active(signal);
      return { printed: true };
    };

    const call = async (
      input: Record<string, unknown>,
      signal: AbortSignal,
    ): Promise<unknown> => {
      active(signal);
      if (role === "provider") return provider(input);
      switch (input.op) {
        case "recipientKey":
          return recipientKey(input, signal);
        case "request":
          return request(input, signal);
        case "open":
          return open(input);
        case "release":
          return release(input);
        case "display":
          return display(input, signal);
        case "print":
          return print(input, signal);
      }
      throw new LocketError("unsupported", "Operation not supported.");
    };
    // `request`s wait for each other, so the user sees one sheet at a time
    // per app. Every other call runs at once: `display` and `release` do not
    // wait behind an open sheet. `{ cancel: id }` withdraws a call not yet
    // answered, queued or running: it is answered `timeout` at once, any
    // sheet it opened closes, and it keeps nothing it would have made. A
    // call already answered is not withdrawn; its answer went first.
    port.onmessage = (event: MessageEvent<unknown>) => {
      const input = event.data;
      if (typeof input !== "object" || input === null) return;
      const record = input as Record<string, unknown>;
      if (typeof record.cancel === "number") {
        const withdrawn = pending.get(record.cancel);
        if (withdrawn === undefined) return;
        pending.delete(record.cancel);
        withdrawn.abort();
        port.postMessage({
          id: record.cancel,
          error: {
            code: "timeout",
            message: "The app stopped waiting for this.",
          } satisfies LocketErrorReply,
        });
        return;
      }
      if (typeof record.id !== "number" || !Number.isSafeInteger(record.id))
        return;
      const id = record.id;
      if (pending.has(id)) return;
      if (
        pending.size >= MAX_PENDING &&
        !(role === "consumer" && record.op === "release")
      ) {
        port.postMessage({
          id,
          error: {
            code: "busy",
            message: "Too many calls at once. Wait for some to answer.",
          } satisfies LocketErrorReply,
        });
        return;
      }
      const withdrawn = new AbortController();
      pending.set(id, withdrawn);
      const signal = AbortSignal.any([controller.signal, withdrawn.signal]);
      const run = async () => {
        try {
          if (signal.aborted) return;
          const result = await call(record, signal);
          active(signal);
          port.postMessage({ id, result });
        } catch (error) {
          if (!signal.aborted)
            port.postMessage({ id, error: errorReply(error) });
        } finally {
          if (pending.get(id) === withdrawn) pending.delete(id);
        }
      };
      if (role === "consumer" && record.op === "request")
        requests = requests.then(run);
      else void run();
    };
    port.start();
    port.postMessage({ protocol: LOCKET_PROTOCOL, role });
    return close;
  }
}
