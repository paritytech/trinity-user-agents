import { expect, test } from "bun:test";
import { ProductId, asProductId } from "./generated/types.js";
import { str } from "./scale.js";

test("product IDs preserve string values and wire encoding", () => {
    const value = " PEOPL.DOT ";
    const productId = asProductId(value);
    expect({
        productId,
        encoded: ProductId.enc(productId),
        decoded: ProductId.dec(str.enc(value)),
        cloned: structuredClone(productId),
    }).toEqual({
        productId: value,
        encoded: str.enc(value),
        decoded: value,
        cloned: value,
    });
});

test("product ID APIs reject ordinary strings", async () => {
    const { default: ts } = await import("typescript");
    const fixture = new URL("../tests/product-id.types.ts", import.meta.url);
    const program = ts.createProgram([fixture.pathname], {
        noEmit: true,
        strict: true,
        skipLibCheck: true,
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
    });
    const diagnostics = ts.getPreEmitDiagnostics(program);
    expect(
        diagnostics.map((diagnostic) =>
            ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
        ),
    ).toEqual([]);
});
