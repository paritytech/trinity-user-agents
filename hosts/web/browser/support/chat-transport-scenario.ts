/**
 * A chat exchange for the inspector's browser test, run inside the page:
 * Alice and Bob, with synthetic phrases, over an in-memory JSON-RPC statement
 * store, each through a real StatementStore and ChatEngine. Only Bob's side is
 * recorded and captured. Nothing leaves the page.
 */
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { bytes, concat, text, u64 } from "../../src/chat/nova/codec.js";
import { aeadKey, cryptoRandom } from "../../src/chat/nova/crypto.js";
import type { ChatDiagnostics } from "../../tools/inspector/diagnostics.js";
import type { ChainDirectory } from "../../src/chat/nova/directory.js";
import { ChatEngine } from "../../src/chat/nova/engine.js";
import { localNovaIdentity } from "../../src/chat/nova/identity.js";
import { ChatKeys, deriveChatSecret } from "../../src/chat/nova/keys.js";
import { MemoryStatementRpc } from "../../fixtures/nova/memory-rpc.js";
import {
  encodeSessionData,
  sealSessionPlaintext,
  sessionTopic,
} from "../../src/chat/nova/protocol.js";
import {
  encodeSigned,
  expiryAt,
  statementHash,
} from "../../src/chat/nova/statement.js";
import { StatementStore } from "../../src/chat/nova/statement-store.js";
import { MemoryStorage } from "../../tests/support/test-support.js";

const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const settle = () => new Promise((resolve) => setTimeout(resolve, 40));

/** What the test checks the inspector against. */
export interface ScenarioValues {
  control: string;
  controlPlaintext: string;
  token: string;
  deviceAccount: string;
  deviceKey: string;
  garbled: string;
  garbledHash: string;
  /** Values that must never appear anywhere in the inspector. */
  secrets: string[];
}

export async function runScenario(
  diagnostics: ChatDiagnostics,
): Promise<ScenarioValues> {
  const entropy = (phrase: string) => mnemonicToEntropy(phrase, wordlist);
  const alice = ChatKeys.fromEntropy(entropy(ALICE), "paseo");
  const bob = ChatKeys.fromEntropy(entropy(BOB), "paseo");
  const people = [
    { label: "alice.01", keys: alice },
    { label: "bob.02", keys: bob },
  ];
  const find = (account: Uint8Array) =>
    people.find(
      (person) => bytesToHex(person.keys.identity) === bytesToHex(account),
    );
  const chain: ChainDirectory = {
    async names(account) {
      const person = find(account);
      return person === undefined
        ? null
        : {
            lite: { label: person.label, chatKey: person.keys.chatPublic },
            full: null,
          };
    },
    async consumer() {
      return null;
    },
    async owners(label) {
      const person = people.find((candidate) => candidate.label === label);
      return { assetHub: person?.keys.identity ?? null, people: null };
    },
    async hasAllowance() {
      return true;
    },
  };
  const rpc = new MemoryStatementRpc();
  const engine = (keys: ChatKeys, captured: boolean) =>
    new ChatEngine({
      identity: localNovaIdentity(keys),
      openStore: async () =>
        new StatementStore(
          rpc.connect(),
          captured ? diagnostics.payloads.connectionTap() : undefined,
        ),
      directory: chain,
      storage: new MemoryStorage(),
      recordKey: "chat",
      writes: () => true,
      ...(captured
        ? { diagnostics: diagnostics.record, payloads: diagnostics.payloads }
        : {}),
    });
  const aliceEngine = engine(alice, false);
  const bobEngine = engine(bob, true);
  await Promise.all([aliceEngine.start(), bobEngine.start()]);
  await aliceEngine.startChat("bob.02", "hello from alice");
  await settle();
  await bobEngine.accept(bobEngine.snapshot().requests[0].id);
  await settle();
  await aliceEngine.send(
    aliceEngine.snapshot().contacts[0].account,
    "how are you",
  );
  await settle();

  const shared = alice.agree(bob.chatPublic);
  const topic = sessionTopic(shared, alice.identity, bob.identity);
  const send = async (data: Uint8Array) => {
    const encoded = encodeSigned(
      { expiry: expiryAt(Date.now()), channel: null, topics: [topic], data },
      alice.identity,
      (payload) => alice.sign(payload),
    );
    const store = new StatementStore(rpc.connect());
    await store.submit(encoded);
    store.close();
    await settle();
    return encoded;
  };
  const token = Uint8Array.of(0xfe, 0xed, 0xfa, 0xce, 0x10, 0x20, 0x30, 0x40);
  const deviceAccount = new Uint8Array(32).fill(0x31);
  const deviceKey = new Uint8Array(32).fill(0x42);
  const head = (id: string, kind: number) =>
    concat(text(id), u64(1_760_000_000_000n), Uint8Array.of(0, kind));
  const controlPlaintext = encodeSessionData({
    kind: "request",
    requestId: "control-request",
    messages: [
      concat(head("c1", 1), bytes(token), Uint8Array.of(1)),
      concat(head("c2", 17), bytes(deviceAccount), bytes(deviceKey)),
    ],
  });
  const control = await send(
    sealSessionPlaintext(shared, controlPlaintext, cryptoRandom),
  );
  const garbled = await send(new Uint8Array(40).fill(9));

  // A reconnect: the store delivers what it holds again.
  bobEngine.stop();
  await bobEngine.start();
  await settle();
  bobEngine.stop();
  aliceEngine.stop();

  return {
    control: bytesToHex(control),
    controlPlaintext: bytesToHex(controlPlaintext),
    token: bytesToHex(token),
    deviceAccount: bytesToHex(deviceAccount),
    deviceKey: bytesToHex(deviceKey),
    garbled: bytesToHex(garbled),
    garbledHash: bytesToHex(statementHash(garbled)),
    secrets: [
      bytesToHex(shared),
      bytesToHex(aeadKey(shared)),
      bytesToHex(entropy(ALICE)),
      bytesToHex(entropy(BOB)),
      bytesToHex(deriveChatSecret(entropy(BOB))),
      bytesToHex(deriveChatSecret(entropy(ALICE))),
    ].map((value) => value.slice(2)),
  };
}
