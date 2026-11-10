import type { ProductId, TrUApiClient } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import type { WireSignedStatement } from "../../bridges/nova-chat/contract.js";
import type { NovaStatements } from "./session.js";

/**
 * PROTOTYPE. Nova Chat's statements through the product's own TrUAPI
 * `statementStore`: it follows its conversations' topics, and signs with
 * `createProof` as the product's account at a conversation's raw derivation
 * index, whose key the core holds. A statement it reads keeps its sr25519
 * proof, which the host checks again before it opens the frame.
 */

/** How long a subscription that ended waits before it starts again. */
const FOLLOW_RETRY_MS = 15_000;

type StatementClient = Pick<TrUApiClient, "statementStore">;

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  let value = error;
  for (let depth = 0; depth < 3; depth++) {
    if (typeof value !== "object" || value === null) break;
    const { reason, value: inner } = value as {
      reason?: unknown;
      value?: unknown;
    };
    if (typeof reason === "string") return reason;
    value = inner;
  }
  const kind = (error as { tag?: unknown } | null)?.tag;
  return typeof kind === "string"
    ? `the host answered ${kind}`
    : "unknown error";
}

export function novaStatements(
  client: StatementClient,
  productId: ProductId,
  schedule: (task: () => void, delayMs: number) => () => void,
): NovaStatements {
  return {
    follow(topics, deliver, problem) {
      let stopped = false;
      let current: { unsubscribe(): void } | null = null;
      let cancelRetry = () => {};
      const again = (reason: string) => {
        current = null;
        if (stopped) return;
        problem(reason);
        cancelRetry = schedule(start, FOLLOW_RETRY_MS);
      };
      function start(): void {
        if (stopped || topics.length === 0) return;
        current = client.statementStore
          .subscribe({
            request: { tag: "MatchAny", value: topics as HexString[] },
          })
          .subscribe({
            next: (page) =>
              deliver(
                page.statements.flatMap((statement): WireSignedStatement[] =>
                  statement.proof.tag === "Sr25519" &&
                  statement.expiry !== undefined &&
                  statement.decryptionKey === undefined
                    ? [
                        {
                          signer: statement.proof.value.signer,
                          signature: statement.proof.value.signature,
                          expiry: statement.expiry.toString(),
                          channel: statement.channel ?? null,
                          topics: statement.topics,
                          data: statement.data ?? "0x",
                        },
                      ]
                    : [],
                ),
              ),
            error: (error) => again(errorText(error)),
            complete: () => again("the subscription ended"),
          });
      }
      start();
      return () => {
        stopped = true;
        cancelRetry();
        current?.unsubscribe();
      };
    },

    async post(index, wire) {
      const statement = {
        expiry: BigInt(wire.expiry),
        ...(wire.channel === null
          ? {}
          : { channel: wire.channel as HexString }),
        topics: wire.topics as HexString[],
        data: wire.data as HexString,
      };
      const proof = await client.statementStore.createProof({
        productAccountId: {
          dotNsIdentifier: productId,
          derivationIndex: { tag: "Raw", value: index as HexString },
        },
        statement,
      });
      if (proof.isErr()) throw new Error(errorText(proof.error));
      const result = await client.statementStore.submit({
        ...statement,
        proof: proof.value.proof,
      });
      if (result.isErr()) throw new Error(errorText(result.error));
    },
  };
}
