import type { Result } from "neverthrow";

/** A failure of a command, with the reason of its `error` reply. */
export class CommandError extends Error {}

function field(value: unknown, key: string): unknown {
  return value !== null && typeof value === "object" && key in value
    ? (value as Record<string, unknown>)[key]
    : undefined;
}

/** The variant name of an error value, with its reason if it has one. */
function describeVariant(value: unknown): string {
  const tag = field(value, "tag");
  if (typeof tag !== "string") return String(value);
  const reason = field(field(value, "value"), "reason");
  return typeof reason === "string" ? `${tag} (${reason})` : tag;
}

/**
 * One line for a `CallError`: the domain variant inside its version wrapper,
 * such as `UnknownMessage`, or the call-level variant, such as `Unsupported`.
 */
export function describeCallError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (field(error, "tag") === "Domain") {
    const versioned = field(error, "value");
    return describeVariant(field(versioned, "value") ?? versioned);
  }
  return describeVariant(error);
}

/** Wait for a call and give its value, or throw a `CommandError`. */
export async function call<T, E>(method: string, result: PromiseLike<Result<T, E>>): Promise<T> {
  const outcome = await result;
  if (outcome.isErr()) throw new CommandError(`${method} ${describeCallError(outcome.error)}`);
  return outcome.value;
}

export function reasonOf(error: unknown): string {
  return error instanceof Error ? error.message : describeCallError(error);
}
