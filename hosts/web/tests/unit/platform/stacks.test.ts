import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  decodeLink,
  encodeLink,
  publicKey,
  sign,
  type Descriptor,
} from "@parity/trinity-stack";
import {
  BUILT_IN_STACKS,
  NETWORKS,
  bundledCopy,
  type BuiltInStack,
} from "../../../src/platform/network-config.js";
import {
  activeStack,
  checkCustomStack,
  customRecordId,
  customStacks,
  forgetCustomStack,
  refreshCustomStack,
  refreshStack,
  saveCustomStack,
  startupCustomStack,
  startupStack,
  storedStackKey,
} from "../../../src/platform/stacks.js";
import { MemoryStorage } from "../../support/test-support.js";

const URL_ = "https://stack.example/.well-known/trinity-stack.json";
const SECRET = new Uint8Array(32).fill(7);
const OTHER_SECRET = new Uint8Array(32).fill(9);
const NOW = new Date("2026-11-11T12:00:00Z");

/** Paseo's bundled descriptor at `sequence`, changed by `edit` and signed by `secret`. */
function copy(
  sequence: number,
  edit: (descriptor: Descriptor) => void = () => {},
  secret = SECRET,
): Uint8Array {
  const descriptor = structuredClone(
    bundledCopy(BUILT_IN_STACKS.paseo).descriptor,
  );
  descriptor.sequence = sequence;
  edit(descriptor);
  return sign(new TextEncoder().encode(JSON.stringify(descriptor)), secret);
}

/** Paseo as a built-in signed by the test key, bundling `bundled`. */
function builtIn(bundled: Uint8Array = copy(100)): BuiltInStack {
  return {
    ...BUILT_IN_STACKS.paseo,
    pointer: encodeLink({ url: URL_, key: publicKey(SECRET) }),
    bundled,
  };
}

const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

const moveRpc = (descriptor: Descriptor) => {
  descriptor.chains[2].rpc = ["wss://people-moved.example"];
};

/** A server that answers every fetch with `served`. */
const serving = (served: Uint8Array) => async () =>
  new Response(served as BodyInit, { status: 200 });

describe("startupStack", () => {
  test("starts from the bundled copy when nothing is stored", () => {
    const stack = builtIn();
    const startup = startupStack(stack, new MemoryStorage(), NOW);
    expect([startup.source, startup.problem]).toEqual(["bundled", null]);
    expect(startup.config.stack.document).toEqual(stack.bundled);
  });

  // A tab never waits on the server: the last copy fetched is used at once.
  test("starts from a newer stored copy, reading what it says", () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    storage.setItem(storedStackKey(stack), text(copy(101, moveRpc)));
    const startup = startupStack(stack, storage, NOW);
    expect([startup.source, startup.config.chains.people.rpc]).toEqual([
      "stored",
      "wss://people-moved.example",
    ]);
  });

  // A build that ships a newer copy than the one stored uses its own.
  test("prefers a bundled copy newer than the stored one", () => {
    const stack = builtIn(copy(102));
    const storage = new MemoryStorage();
    storage.setItem(storedStackKey(stack), text(copy(101, moveRpc)));
    const startup = startupStack(stack, storage, NOW);
    expect([startup.source, startup.config.stack.descriptor.sequence]).toEqual([
      "bundled",
      102,
    ]);
  });

  // The key in the link is the trust anchor, so another key's copy is no copy.
  test("ignores a stored copy signed by another key", () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    storage.setItem(
      storedStackKey(stack),
      text(copy(101, moveRpc, OTHER_SECRET)),
    );
    const startup = startupStack(stack, storage, NOW);
    expect([startup.source, startup.config.chains.people.rpc]).toEqual([
      "bundled",
      "wss://paseo-people-next-system-rpc.polkadot.io",
    ]);
  });

  // Every account derives under the TLD, so a copy that moves it never runs.
  test("passes over a stored copy that changes the TLD, and says why", () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    storage.setItem(
      storedStackKey(stack),
      text(
        copy(101, (descriptor) => {
          descriptor.tld = "testnet";
        }),
      ),
    );
    const startup = startupStack(stack, storage, NOW);
    expect(startup.source).toBe("bundled");
    expect(startup.config.networkSuffix).toBe("paseo");
    expect(startup.problem).toContain("changes the TLD from paseo to testnet");
  });
});

describe("refreshStack", () => {
  test("stores a newer published copy for the next load, and says it moves what the host reads", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    const published = copy(101, moveRpc);

    const refresh = await refreshStack(stack, running, storage, {
      fetch: serving(published),
      now: NOW,
    });

    expect(refresh).toEqual({
      outcome: "stored",
      sequence: 101,
      changes: true,
      newGeneration: false,
    });
    expect(storage.getItem(storedStackKey(stack))).toBe(text(published));
    expect(startupStack(stack, storage, NOW).config.chains.people.rpc).toBe(
      "wss://people-moved.example",
    );
  });

  test("reports restarted chains", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    const refresh = await refreshStack(stack, running, storage, {
      fetch: serving(
        copy(101, (descriptor) => {
          descriptor.generation = "restarted";
        }),
      ),
      now: NOW,
    });
    expect(refresh).toMatchObject({ outcome: "stored", newGeneration: true });
  });

  test("keeps nothing when the published copy is the one in use", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    expect(
      await refreshStack(stack, running, storage, {
        fetch: serving(stack.bundled),
        now: NOW,
      }),
    ).toEqual({ outcome: "current", status: "unchanged" });
    expect(storage.length).toBe(0);
  });

  // A server replaying an older signed copy cannot move the host back.
  test("refuses an older signed copy", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    expect(
      await refreshStack(stack, running, storage, {
        fetch: serving(copy(99, moveRpc)),
        now: NOW,
      }),
    ).toEqual({ outcome: "current", status: "rollback" });
    expect(storage.length).toBe(0);
  });

  test("refuses, and stores nothing of, a copy that changes the id", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    const refresh = await refreshStack(stack, running, storage, {
      fetch: serving(
        copy(101, (descriptor) => {
          descriptor.id = "paseo-fork";
        }),
      ),
      now: NOW,
    });
    expect(refresh.outcome).toBe("refused");
    expect(storage.length).toBe(0);
  });

  test("refuses a copy signed by another key", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    expect(
      await refreshStack(stack, running, storage, {
        fetch: serving(copy(101, moveRpc, OTHER_SECRET)),
        now: NOW,
      }),
    ).toEqual({
      outcome: "refused",
      reason: "the published copy breaks the rule bad-signature",
    });
  });

  // Another tab may have stored a newer copy since this one started.
  test("never replaces a newer copy another tab stored", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    const newer = text(copy(102));
    storage.setItem(storedStackKey(stack), newer);
    expect(
      await refreshStack(stack, running, storage, {
        fetch: serving(copy(101, moveRpc)),
        now: NOW,
      }),
    ).toEqual({ outcome: "current", status: "rollback" });
    expect(storage.getItem(storedStackKey(stack))).toBe(newer);
  });

  test("keeps the copy in use when the server cannot be reached", async () => {
    const stack = builtIn();
    const storage = new MemoryStorage();
    const running = startupStack(stack, storage, NOW).config;
    expect(
      await refreshStack(stack, running, storage, {
        fetch: async () => {
          throw new TypeError("offline");
        },
        now: NOW,
      }),
    ).toEqual({ outcome: "unreachable" });
  });
});

const CUSTOM_URL = "https://custom.example/.well-known/trinity-stack.json";

/** A descriptor for a custom stack: Paseo's, renamed, on its own chains. */
function customDescriptor(
  sequence: number,
  edit: (descriptor: Descriptor) => void = () => {},
): Descriptor {
  const descriptor = structuredClone(
    bundledCopy(BUILT_IN_STACKS.paseo).descriptor,
  );
  descriptor.id = "staging";
  descriptor.name = "Staging";
  descriptor.sequence = sequence;
  descriptor.chains.forEach((chain, index) => {
    chain.genesisHash = `0x${String(index + 1).repeat(64)}`;
  });
  edit(descriptor);
  return descriptor;
}

const signedCustom = (sequence: number, edit?: (d: Descriptor) => void) =>
  sign(
    new TextEncoder().encode(JSON.stringify(customDescriptor(sequence, edit))),
    SECRET,
  );
const plainCustom = (sequence: number, edit?: (d: Descriptor) => void) =>
  new TextEncoder().encode(JSON.stringify(customDescriptor(sequence, edit)));

const signedLink = encodeLink({ url: CUSTOM_URL, key: publicKey(SECRET) });

describe("checkCustomStack", () => {
  test("reads a stack link's descriptor, pinned to its key, without keeping it", async () => {
    const check = await checkCustomStack(signedLink, {
      fetch: serving(signedCustom(5)),
      now: NOW,
    });
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    expect([
      check.signed,
      check.config.origin,
      check.config.displayName,
      check.config.networkSuffix,
      check.config.backup,
      check.record.pinned,
    ]).toEqual([
      true,
      "custom",
      "Staging (custom)",
      "paseo",
      false,
      { id: "staging", tld: "paseo" },
    ]);
    expect(decodeLink(check.record.link).key).toBe(publicKey(SECRET));
  });

  // A plain URL pins nobody: the record keeps no key, and the copy is unsigned.
  test("reads a plain URL as an unsigned stack", async () => {
    const check = await checkCustomStack(CUSTOM_URL, {
      fetch: serving(plainCustom(5)),
      now: NOW,
    });
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    expect([check.signed, decodeLink(check.record.link)]).toEqual([
      false,
      { url: CUSTOM_URL },
    ]);
  });

  // The build's pinned key is more than a URL gives, so a built-in is never re-added as custom.
  test("answers a built-in stack's URL or key with the built-in", async () => {
    const paseo = decodeLink(BUILT_IN_STACKS.paseo.pointer);
    const refuse = async () => {
      throw new Error("no fetch for a built-in");
    };
    expect(await checkCustomStack(paseo.url, { fetch: refuse })).toEqual({
      kind: "built-in",
      id: "paseo",
    });
    expect(
      await checkCustomStack(encodeLink({ url: CUSTOM_URL, key: paseo.key }), {
        fetch: refuse,
      }),
    ).toEqual({ kind: "built-in", id: "paseo" });
  });

  test("refuses a copy the link's key did not sign", async () => {
    await expect(
      checkCustomStack(signedLink, {
        fetch: serving(
          sign(
            new TextEncoder().encode(JSON.stringify(customDescriptor(5))),
            OTHER_SECRET,
          ),
        ),
        now: NOW,
      }),
    ).rejects.toThrow("bad-signature");
  });

  test("says why nothing was read when the server does not answer", async () => {
    await expect(
      checkCustomStack(CUSTOM_URL, {
        fetch: async () => {
          throw new TypeError("blocked by CORS");
        },
      }),
    ).rejects.toThrow("does not allow this page to read it (CORS)");
  });

  test("refuses what is not a descriptor URL", async () => {
    await expect(checkCustomStack("not a url")).rejects.toThrow(
      "That is not a stack descriptor URL or stack link",
    );
    await expect(
      checkCustomStack("http://custom.example/stack.json"),
    ).rejects.toThrow("That is not a stack descriptor URL or stack link");
  });

  // PCF's Polkadot stack: Polkadot's chains under the dot TLD, pinned to its own key.
  test("takes the Polkadot stack the CLI ships, as a custom stack", async () => {
    const stacks = new URL(
      "../../../../../rust/crates/truapi-host-cli/stacks/",
      import.meta.url,
    );
    const check = await checkCustomStack(
      readFileSync(new URL("polkadot.link", stacks), "utf8"),
      {
        fetch: serving(
          new Uint8Array(readFileSync(new URL("polkadot.json", stacks))),
        ),
        now: NOW,
      },
    );
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    expect([
      check.signed,
      check.config.networkSuffix,
      check.config.genesis.relay,
    ]).toEqual([
      true,
      "dot",
      "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3",
    ]);
  });

  // A mirror of a built-in at another URL is its own stack, kept apart from the built-in.
  test("takes a copy of a built-in stack served at another URL", async () => {
    const check = await checkCustomStack(CUSTOM_URL, {
      fetch: serving(
        new TextEncoder().encode(
          JSON.stringify(bundledCopy(BUILT_IN_STACKS.paseo).descriptor),
        ),
      ),
      now: NOW,
    });
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    expect([check.config.genesis, check.config.networkSuffix]).toEqual([
      NETWORKS.paseo.genesis,
      "paseo",
    ]);
    expect(check.config.id).not.toBe("paseo");
  });

  test("takes a custom stack on Polkadot's TLD", async () => {
    const check = await checkCustomStack(CUSTOM_URL, {
      fetch: serving(
        plainCustom(5, (descriptor) => {
          descriptor.tld = "dot";
        }),
      ),
      now: NOW,
    });
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    expect(check.config.networkSuffix).toBe("dot");
  });

  test("refuses an expired stack", async () => {
    await expect(
      checkCustomStack(CUSTOM_URL, {
        fetch: serving(
          plainCustom(5, (descriptor) => {
            descriptor.expiresAt = Math.floor(NOW.getTime() / 1000) - 60;
          }),
        ),
        now: NOW,
      }),
    ).rejects.toThrow("past its expiresAt");
  });
});

describe("custom stacks", () => {
  async function added(storage: MemoryStorage, link = signedLink) {
    const check = await checkCustomStack(link, {
      fetch: serving(link === signedLink ? signedCustom(5) : plainCustom(5)),
      now: NOW,
    });
    if (check.kind !== "custom") throw new Error("expected a custom stack");
    saveCustomStack(storage, check.record);
    return check;
  }

  test("a kept stack starts from its kept copy, under its own namespace", async () => {
    const storage = new MemoryStorage();
    const check = await added(storage);
    const choice = activeStack(check.config.id, storage, NOW);
    if (choice.status !== "ready") throw new Error(choice.reason);
    expect([choice.startup.config.id, choice.startup.source]).toEqual([
      check.config.id,
      "stored",
    ]);
    expect(customStacks(storage)).toHaveLength(1);
  });

  test("a refresh keeps a newer copy, and refuses one that changes the TLD", async () => {
    const storage = new MemoryStorage();
    const check = await added(storage);
    const record = customStacks(storage)[0];
    const running = startupCustomStack(record, storage, NOW).config;
    expect(
      await refreshCustomStack(record, running, storage, {
        fetch: serving(
          signedCustom(6, (descriptor) => {
            descriptor.tld = "testnet";
          }),
        ),
        now: NOW,
      }),
    ).toMatchObject({ outcome: "refused" });
    expect(
      await refreshCustomStack(record, running, storage, {
        fetch: serving(signedCustom(6, moveRpc)),
        now: NOW,
      }),
    ).toMatchObject({ outcome: "stored", sequence: 6, changes: true });
    const choice = activeStack(check.config.id, storage, NOW);
    if (choice.status !== "ready") throw new Error(choice.reason);
    expect(choice.startup.config.chains.people.rpc).toBe(
      "wss://people-moved.example",
    );
  });

  // Only a signed stack has rollback protection.
  test("an unsigned stack takes an older copy its server sends", async () => {
    const storage = new MemoryStorage();
    await added(storage, CUSTOM_URL);
    const record = customStacks(storage)[0];
    const running = startupCustomStack(record, storage, NOW).config;
    expect(
      await refreshCustomStack(record, running, storage, {
        fetch: serving(plainCustom(4, moveRpc)),
        now: NOW,
      }),
    ).toMatchObject({ outcome: "stored", sequence: 4 });
  });

  // The tab is never moved to another stack, so it never runs on one the user did not choose.
  test("a stack that can no longer run fails in place, and a refresh can store one that runs", async () => {
    const storage = new MemoryStorage();
    const check = await added(storage);
    const later = new Date(NOW.getTime() + 1000);
    const record = customStacks(storage)[0];
    saveCustomStack(storage, {
      ...record,
      document: new TextDecoder().decode(
        signedCustom(6, (descriptor) => {
          descriptor.expiresAt = Math.floor(NOW.getTime() / 1000);
        }),
      ),
    });
    const choice = activeStack(check.config.id, storage, later);
    if (choice.status !== "failed") throw new Error("expected a failed stack");
    expect(choice.reason).toContain("past its expiresAt");
    expect(
      await choice.refresh!({ fetch: serving(signedCustom(7)), now: later }),
    ).toMatchObject({ outcome: "stored", sequence: 7 });
    expect(activeStack(check.config.id, storage, later)).toMatchObject({
      status: "ready",
      startup: { config: { id: check.config.id } },
    });
  });

  test("forgetting a stack leaves the others", async () => {
    const storage = new MemoryStorage();
    const signed = await added(storage);
    const unsigned = await added(storage, CUSTOM_URL);
    forgetCustomStack(storage, signed.config.id);
    expect(customStacks(storage).map(customRecordId)).toEqual([
      unsigned.config.id,
    ]);
    expect(activeStack(signed.config.id, storage, NOW)).toEqual({
      status: "failed",
      reason: `No stack ${signed.config.id} is kept in this browser.`,
      refresh: null,
    });
  });
});
