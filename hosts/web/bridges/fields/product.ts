import {
  FIELDS_OFFER,
  FIELDS_PROTOCOL,
  FieldError,
  type ConsumerCalls,
  type FieldErrorReply,
  type FieldRole,
  type ProviderCalls,
} from "./protocol.js";

/**
 * DRAFT, web-only. The product end of the field bridge: wait for the port the
 * host offers this document, then call it. Each call answers with its result
 * or rejects with a {@link FieldError}.
 *
 * On the port, a call is `{ ...input, op, id }` and its answer `{ id, result }`
 * or `{ id, error }`. `{ cancel: id }` withdraws a call not answered yet: the
 * host stops its pending work, closes any sheet it opened, keeps nothing it
 * would have returned, and answers it `timeout` at once. Work already done
 * stays done. Answers keep their order, so a result the
 * host sent before it saw the cancel still arrives first, and is the answer.
 */

/** A typed bridge: one method per operation of `Calls`. */
export type FieldClient<Calls> = {
  [Op in keyof Calls]: Calls[Op] extends (input: infer Input) => infer Output
    ? (input: Input) => Promise<Output>
    : never;
};

type Calls<Role extends FieldRole> = Role extends "consumer"
  ? ConsumerCalls
  : ProviderCalls;

/** How long a call waits for the host, which may be showing the user a sheet. */
export const CALL_TIMEOUT_MS = 180_000;
/** How long a cancelled call waits for the host's answer before it gives up. */
const CANCEL_GRACE_MS = 10_000;
const OFFER_TIMEOUT_MS = 15_000;

const OPS: Readonly<Record<FieldRole, readonly string[]>> = {
  consumer: [
    "createReceiveKey",
    "request",
    "open",
    "release",
    "display",
    "print",
  ],
  provider: ["status", "declare", "save"],
};

/** How a client over a port behaves; tests shorten the timeout. */
export interface FieldClientOptions {
  /** See every message the host sends. */
  observe?(message: unknown): void;
  /** How long a call waits before it is cancelled with `timeout`. */
  timeoutMs?: number;
}

/**
 * A client for `role` over `port`, once the host has answered the offer.
 * A call still unanswered after `timeoutMs` is cancelled at the host. It
 * then settles with the host's first answer: `timeout`, or the result the
 * host had already sent. With no answer at all it rejects with `timeout`
 * after a short grace.
 */
export function fieldClient<Role extends FieldRole>(
  port: MessagePort,
  role: Role,
  options: FieldClientOptions = {},
): FieldClient<Calls<Role>> {
  const timeoutMs = options.timeoutMs ?? CALL_TIMEOUT_MS;
  let nextId = 1;
  const waiting = new Map<
    number,
    { resolve(value: unknown): void; reject(error: Error): void }
  >();
  port.addEventListener("message", (message: MessageEvent<unknown>) => {
    const data = message.data as {
      id?: unknown;
      error?: FieldErrorReply;
      result?: unknown;
    } | null;
    if (typeof data?.id !== "number") return;
    const pending = waiting.get(data.id);
    waiting.delete(data.id);
    if (data.error !== undefined)
      pending?.reject(new FieldError(data.error.code, data.error.message));
    else pending?.resolve(data.result);
  });
  port.start();
  const call = (op: string, input: object): Promise<unknown> =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      let grace: ReturnType<typeof setTimeout> | undefined;
      const timer = setTimeout(() => {
        port.postMessage({ cancel: id });
        grace = setTimeout(() => {
          waiting.delete(id);
          reject(new FieldError("timeout", "The host did not answer in time."));
        }, CANCEL_GRACE_MS);
      }, timeoutMs);
      const done = () => {
        clearTimeout(timer);
        clearTimeout(grace);
      };
      waiting.set(id, {
        resolve: (value) => {
          done();
          resolve(value);
        },
        reject: (error) => {
          done();
          reject(error);
        },
      });
      port.postMessage({ ...input, op, id });
    });
  return Object.fromEntries(
    OPS[role].map((op) => [op, (input: object) => call(op, input)]),
  ) as FieldClient<Calls<Role>>;
}

/**
 * Connect this document to the host's field bridge in `role`. Rejects when
 * the page is not inside a host, the host offers no bridge, or it gave this
 * page another role.
 */
export async function connectFields<Role extends FieldRole>(
  role: Role,
  options: FieldClientOptions = {},
): Promise<FieldClient<Calls<Role>>> {
  if (parent === window) throw new Error("Open this page inside the host.");
  const observe = options.observe ?? (() => {});
  const port = await new Promise<MessagePort>((resolve, reject) => {
    const timer = setTimeout(
      () =>
        reject(
          new FieldError(
            "unsupported",
            "The host did not offer the field bridge. Sign in, then reopen this product.",
          ),
        ),
      OFFER_TIMEOUT_MS,
    );
    // Registered before the first await, so the offer cannot come first.
    const onOffer = (event: MessageEvent<unknown>) => {
      const offer = event.data as { type?: unknown; protocol?: unknown } | null;
      if (
        event.source !== parent ||
        offer?.type !== FIELDS_OFFER ||
        offer.protocol !== FIELDS_PROTOCOL ||
        event.ports.length !== 1
      )
        return;
      window.removeEventListener("message", onOffer);
      const offered = event.ports[0]!;
      offered.addEventListener("message", (message: MessageEvent<unknown>) => {
        observe(message.data);
        const data = message.data as {
          protocol?: unknown;
          role?: unknown;
        } | null;
        if (data?.protocol !== FIELDS_PROTOCOL) return;
        clearTimeout(timer);
        if (data.role === role) resolve(offered);
        else
          reject(
            new FieldError(
              "unsupported",
              "This app is not registered for that role.",
            ),
          );
      });
      offered.start();
    };
    window.addEventListener("message", onOffer);
  });
  return fieldClient(port, role, options);
}
