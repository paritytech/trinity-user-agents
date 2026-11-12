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
const CALL_TIMEOUT_MS = 180_000;
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

/**
 * Connect this document to the host's field bridge in `role`. Rejects when
 * the page is not inside a host, the host offers no bridge, or it gave this
 * page another role. `observe` sees every message the host sends.
 */
export async function connectFields<Role extends FieldRole>(
  role: Role,
  observe: (message: unknown) => void = () => {},
): Promise<FieldClient<Calls<Role>>> {
  if (parent === window) throw new Error("Open this page inside the host.");
  let nextId = 1;
  const waiting = new Map<
    number,
    { resolve(value: unknown): void; reject(error: Error): void }
  >();
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
      offered.onmessage = (message: MessageEvent<unknown>) => {
        observe(message.data);
        const data = message.data as {
          protocol?: unknown;
          role?: unknown;
          id?: unknown;
          error?: FieldErrorReply;
          result?: unknown;
        } | null;
        if (data?.protocol === FIELDS_PROTOCOL) {
          clearTimeout(timer);
          if (data.role === role) resolve(offered);
          else
            reject(
              new FieldError(
                "unsupported",
                "This app is not registered for that role.",
              ),
            );
          return;
        }
        if (typeof data?.id !== "number") return;
        const pending = waiting.get(data.id);
        waiting.delete(data.id);
        if (data.error !== undefined)
          pending?.reject(new FieldError(data.error.code, data.error.message));
        else pending?.resolve(data.result);
      };
    };
    window.addEventListener("message", onOffer);
  });
  const call = (op: string, input: object): Promise<unknown> =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        waiting.delete(id);
        reject(new FieldError("busy", "The host did not answer in time."));
      }, CALL_TIMEOUT_MS);
      waiting.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      port.postMessage({ ...input, op, id });
    });
  return Object.fromEntries(
    OPS[role].map((op) => [op, (input: object) => call(op, input)]),
  ) as FieldClient<Calls<Role>>;
}
