import { describe, expect, test } from "bun:test";
import {
  customSummary,
  offeredStacks,
  trustText,
} from "../../../src/app/stack-menu.js";
import {
  BUILT_IN_STACKS,
  NETWORKS,
  bundledCopy,
  customPolicy,
  networkConfigFrom,
} from "../../../src/platform/network-config.js";
import {
  customRecordId,
  saveCustomStack,
} from "../../../src/platform/stacks.js";
import { MemoryStorage } from "../../support/test-support.js";

/** Paseo's descriptor as an unsigned custom stack at `url`. */
function unsignedCustom(url = "https://custom.example/stack.json") {
  const copy = bundledCopy(BUILT_IN_STACKS.paseo);
  const pointer = { url };
  return networkConfigFrom(customPolicy(pointer), {
    ...copy,
    pointer,
  });
}

describe("offeredStacks", () => {
  test("offers the three built-ins, then the custom stacks kept", () => {
    const storage = new MemoryStorage();
    saveCustomStack(storage, {
      link: "polkadotapp://stack?url=https%3A%2F%2Fcustom.example%2Fstack.json",
      document: "{}",
      pinned: { id: "staging", tld: "paseo" },
    });
    expect(offeredStacks(storage)).toEqual([
      { id: "paseo", label: "Paseo Next v2" },
      { id: "previewnet", label: "Previewnet" },
      { id: "pcf-devnet", label: "PCF Devnet" },
      {
        id: customRecordId({
          link: "polkadotapp://stack?url=https%3A%2F%2Fcustom.example%2Fstack.json",
          document: "{}",
          pinned: { id: "staging", tld: "paseo" },
        }),
        label: "staging (custom, custom.example)",
      },
    ]);
  });
});

describe("trustText", () => {
  test("says a built-in's signer is pinned by the build", () => {
    expect(trustText(NETWORKS.paseo)).toEqual([
      "Built in. Every copy must be signed by the key this build pins (0x2b89d8ca…8d5377).",
    ]);
  });

  test("warns that the devnet's dot TLD derives Polkadot identities", () => {
    expect(trustText(NETWORKS["pcf-devnet"])[1]).toContain(
      "a recovery phrase derives the same identity here as on Polkadot",
    );
  });

  test("says who decides an unsigned custom stack", () => {
    expect(trustText(unsignedCustom())[0]).toBe(
      "Custom and unsigned: whoever serves https://custom.example/stack.json decides its chains and services. Every chain runs on its RPC node; backup and username search are off.",
    );
  });
});

describe("customSummary", () => {
  const config = unsignedCustom();
  const check = {
    kind: "custom" as const,
    config,
    record: { link: "", document: "", pinned: { id: "", tld: "" } },
    signed: false,
    stale: false,
  };

  test("warns about an unsigned URL and accounts shared with a built-in on the TLD", () => {
    const rows = customSummary(check, []);
    const row = (label: string) => rows.find((entry) => entry.label === label)!;
    expect(row("Signed by").tone).toBe("warning");
    expect(row("TLD")).toEqual({
      label: "TLD",
      value:
        "paseo. A recovery phrase derives the same accounts here as on Paseo Next v2, and a signature not bound to a chain, such as a statement, is valid on both.",
      tone: "warning",
    });
  });

  test("marks a node serving another chain as an error", () => {
    const rows = customSummary(check, [
      {
        role: "people",
        rpc: config.chains.people.rpc,
        outcome: "wrong",
        detail: "it serves another chain",
      },
    ]);
    expect(rows.find((entry) => entry.label === "People")).toEqual({
      label: "People",
      value: `${config.chains.people.rpc}: wrong chain, it serves another chain`,
      tone: "error",
    });
  });
});
