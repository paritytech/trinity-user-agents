import { describe, expect, test } from "bun:test";
import { okAsync } from "neverthrow";
import { development_createAccountProof } from "./development.js";
import type { HostAccountCreateProofRequest } from "./generated/index.js";

const context = `0x${"ab".repeat(32)}` as const;
const base = {
    keyHandle: { dotNsIdentifier: "dim2.dot", derivationIndex: { tag: "Index", value: 0 } },
    ringLocation: { chainId: `0x${"00".repeat(32)}`, junctions: [] },
    message: "0x01",
} as const;

function stub() {
    const seen: HostAccountCreateProofRequest[] = [];
    const client = {
        account: {
            createAccountProof(request: HostAccountCreateProofRequest) {
                seen.push(request);
                return okAsync({
                    proof: "0x",
                    contextualAlias: { context, alias: "0x" },
                    ringIndex: 0,
                    ringRevision: 0,
                });
            },
        },
    };
    return {
        seen,
        client: client as unknown as Parameters<typeof development_createAccountProof>[0],
    };
}

describe("development_createAccountProof", () => {
    test("forwards the request with the raw context marker", async () => {
        const { seen, client } = stub();
        await development_createAccountProof(client, { ...base, context });
        expect(seen).toEqual([
            { ...base, context: { productId: "raw:", suffix: { tag: "Raw", value: context } } },
        ]);
    });

    test("rejects contexts that are not 32 bytes of hex", () => {
        const { client } = stub();
        for (const bad of ["0x00", "ab".repeat(32), `0x${"zz".repeat(32)}`]) {
            expect(() =>
                development_createAccountProof(client, { ...base, context: bad as `0x${string}` }),
            ).toThrow(TypeError);
        }
    });
});
