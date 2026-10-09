// Cross-product account signing against a real signing-host CLI.
//
// The sibling of `cross-product-ringvrf-e2e.ts`, for the same `context` scope
// on the signing methods rather than on a ring-VRF key. One product signs with
// another product's account, which the owner's manifest grant is the only thing
// permitting. The host resolves that grant from the `trustedProducts` in a
// local product config, so the flow runs before either product is deployed and
// without a chain. See `--product-config` and
// `truapi-host-cli/src/product_config.rs`.
//
// What the assertion can and cannot be. The ring-VRF sibling compares signature
// bytes, because a ring-VRF signature over the same message is the same bytes.
// sr25519 is randomized, so two signatures over one payload differ and
// comparing them here would fail against a correct host. What this pins instead
// is the account: the handle the caller named must resolve to the owner's
// public key and not to the caller's own, which is what a host deriving the
// account from the caller would return. That the owner's key is the one that
// actually signed is pinned in
// `signing_host/tests/cross_product_account.rs`, which verifies the signature
// against a derived keypair.
//
// The runner serves one product per host process, so
// `scripts/cross-product-signing-e2e.sh` invokes this once per phase with
// `E2E_PHASE` set, pointing every run at the same `--base-path` so the key
// parked in one is there for the next.
//
// Phases, and what each proves:
//
//   sign-owner      peopl.paseo signs with its own account and parks its own
//                   public key in its own storage.
//   sign-granted    dim2.paseo signs with peopl.paseo's account. Named in
//                   trustedProducts with `context`, so allowed, and the handle
//                   must resolve to the owner's key rather than dim2's own.
//   sign-untrusted  stash.paseo signs the same handle. Same target, absent
//                   from trustedProducts: refused.
//   sign-again      dim2.paseo signs once more, so a refusal above cannot be
//                   the grant having gone.
//
// Both the granted and the untrusted phase run every surface
// paritytech/platform-issues#20 names, not just the one the deep assertion
// uses: `sign_raw`, `sign_payload`, `create_transaction` and the
// statement-store product proof. The bug that issue reports is a granted
// caller being told `PermissionDenied`, so each surface is checked for exactly
// that, in both directions. A surface can still fail for its own reasons here
// (no chain, a payload the host will not build); what may not happen is the
// refusal.

import type { HostContext, TrUApiClient } from "./script-types.d.ts";

declare const truapi: TrUApiClient;
declare const host: HostContext;

type ProductAccountId = ReturnType<HostContext["productAccount"]>;

const OWNER = "peopl.paseo";
const GRANTED = "dim2.paseo";
const UNTRUSTED = "stash.paseo";

const INDEX = { tag: "Index" as const, value: 0 };
const OWNER_HANDLE = { dotNsIdentifier: OWNER, derivationIndex: INDEX };
/// "granted", as the hex the raw payload takes.
const MESSAGE = "0x6772616e746564";
/// Where the owner phase parks its own public key for comparison.
const PUBLIC_KEY_KEY = "owner-public-key";

const REFUSAL = "PermissionDenied";

function stringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/// The refusal variant, or `null` for anything else.
function refusalTag(error: unknown): string | null {
  const domain = error as { tag?: string; value?: { value?: { tag?: string } } };
  if (domain?.tag !== "Domain") {
    return null;
  }
  return domain.value?.value?.tag ?? null;
}

function expectProduct(expected: string): void {
  if (host.productId !== expected) {
    throw new Error(
      `phase expects --product-id ${expected}, host serves ${host.productId}`,
    );
  }
}

/// Sign `MESSAGE` with `account`, without deciding whether that should work.
async function signWith(account: ProductAccountId) {
  return truapi.signing.signRaw({
    account,
    payload: { tag: "Bytes", value: { bytes: MESSAGE } },
  });
}

/// A fixed genesis, so the gate is reached without resolving a chain. The gate
/// answers before any chain work, which is what lets these run offline.
const GENESIS =
  "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3";

/// A statement a day out, so it is unexpired wherever it is checked.
function statement() {
  const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return { expiry, topics: [`0x${bytes.toHex()}` as `0x${string}`] };
}

/// Every surface paritytech/platform-issues#20 names, each naming `account`.
///
/// `refusal` differs on the statement-store surface: its error enum carries no
/// `PermissionDenied`, and answers `UnknownAccount` both for an account this
/// caller may not use and for one that does not exist, which is the same
/// indistinguishability the other three get from `PermissionDenied`.
const SURFACES: {
  name: string;
  refusal: string;
  call: (
    account: ProductAccountId,
  ) => Promise<{ isOk(): boolean; error?: unknown }>;
}[] = [
  {
    name: "signRaw",
    refusal: REFUSAL,
    call: async (account) => signWith(account),
  },
  {
    name: "signPayload",
    refusal: REFUSAL,
    call: async (account) =>
      truapi.signing.signPayload({
        account,
        payload: {
          blockHash: GENESIS,
          blockNumber: "0x00000000",
          era: "0x00",
          genesisHash: GENESIS,
          method: "0x00003448656c6c6f2c20776f726c6421",
          nonce: "0x00000000",
          signedExtensions: [],
          specVersion: "0x00000000",
          tip: "0x00000000000000000000000000000000",
          transactionVersion: "0x00000000",
          version: 4,
        },
      }),
  },
  {
    name: "createTransaction",
    refusal: REFUSAL,
    call: async (account) =>
      truapi.signing.createTransaction({
        signer: account,
        genesisHash: GENESIS,
        callData: "0x000000",
        extensions: [],
        txExtVersion: 0,
        contacts: [],
      }),
  },
  {
    name: "statementStoreCreateProof",
    refusal: "UnknownAccount",
    call: async (account) =>
      truapi.statementStore.createProof({
        productAccountId: account,
        statement: statement(),
      }),
  },
];

/// Run every surface against `OWNER`'s account and require that the grant is
/// the thing that decides, in whichever direction this phase expects.
async function checkEverySurface(refused: boolean): Promise<void> {
  for (const surface of SURFACES) {
    const result = await surface.call(OWNER_HANDLE);
    const tag = result.isOk() ? null : refusalTag(result.error);
    if (refused) {
      if (tag !== surface.refusal) {
        throw new Error(
          `${host.productId} -> ${OWNER} ${surface.name}: expected ${surface.refusal}, got ${result.isOk() ? "a success" : stringify(result.error)}`,
        );
      }
      continue;
    }
    if (tag === surface.refusal) {
      // The bug platform-issues#20 reports: an authorized caller refused.
      throw new Error(
        `${host.productId} was granted ${OWNER} context but ${surface.name} answered ${surface.refusal}`,
      );
    }
  }
  const verb = refused ? "refused" : "admitted";
  console.log(`${verb} on every surface: ${SURFACES.map((s) => s.name).join(", ")}`);
}

/// The public key behind a handle, as this product is allowed to see it.
async function publicKeyOf(
  account: ProductAccountId,
): Promise<`0x${string}`> {
  const read = await truapi.account.getAccount({ productAccountId: account });
  if (!read.isOk()) {
    throw new Error(
      `${host.productId} could not read ${account.dotNsIdentifier}'s account: ${stringify(read.error)}`,
    );
  }
  return read.value.account.publicKey;
}

/// Sign with `OWNER`'s account and require the owner's key behind the handle.
async function expectGrantedSignature(): Promise<void> {
  const signed = await signWith(OWNER_HANDLE);
  if (!signed.isOk()) {
    throw new Error(
      `${host.productId} was granted ${OWNER} context but refused: ${stringify(signed.error)}`,
    );
  }
  const signature = signed.value.signature;
  if (signature.length !== 2 + 64 * 2) {
    throw new Error(
      `expected a 64-byte signature, got ${signature.length} chars: ${signature}`,
    );
  }

  // Read what the owner parked, through the storage grant, and require the
  // handle to have resolved to that key. A host deriving the account from the
  // caller rather than the handle owner still returns a valid signature, and
  // only this comparison catches it.
  const stored = await truapi.localStorage.read({
    product: OWNER,
    key: PUBLIC_KEY_KEY,
  });
  if (!stored.isOk() || !stored.value.value) {
    throw new Error(
      `could not read ${OWNER}'s own public key back: ${stringify(stored.isOk() ? stored.value : stored.error)}`,
    );
  }
  const ownerKey = stored.value.value;
  const named = await publicKeyOf(OWNER_HANDLE);
  if (named !== ownerKey) {
    throw new Error(
      `the handle did not resolve to ${OWNER}: got ${named}, owner is ${ownerKey}`,
    );
  }
  const own = await publicKeyOf(host.productAccount());
  if (own === ownerKey) {
    // Then the comparison above proves nothing: make that a failure rather
    // than a pass that happens to hold.
    throw new Error(
      `${host.productId}'s own account is ${OWNER}'s, so this phase cannot tell them apart`,
    );
  }
  console.log(
    `granted ${host.productId} -> ${OWNER}: signed as ${ownerKey.slice(0, 18)}, not ${own.slice(0, 18)}`,
  );
}

/// Sign with `OWNER`'s account and require the standard refusal.
async function expectRefused(): Promise<void> {
  const signed = await signWith(OWNER_HANDLE);
  if (signed.isOk()) {
    throw new Error(
      `${host.productId} signed with ${OWNER}'s account without a grant: ${signed.value.signature}`,
    );
  }
  const tag = refusalTag(signed.error);
  if (tag !== REFUSAL) {
    // Anything else leaks why it failed, which is what one refusal prevents.
    throw new Error(
      `${host.productId} -> ${OWNER}: expected ${REFUSAL}, got ${stringify(signed.error)}`,
    );
  }
  console.log(`refused ${host.productId} -> ${OWNER}: ${REFUSAL}`);
}

const phase = process.env.E2E_PHASE;

switch (phase) {
  case "sign-owner": {
    expectProduct(OWNER);
    const signed = await signWith(host.productAccount());
    if (!signed.isOk()) {
      throw new Error(
        `the owner could not sign with its own account: ${stringify(signed.error)}`,
      );
    }
    const ownerKey = await publicKeyOf(host.productAccount());
    const written = await truapi.localStorage.write({
      key: PUBLIC_KEY_KEY,
      value: ownerKey,
    });
    if (!written.isOk()) {
      throw new Error(
        `could not park the owner public key: ${stringify(written.error)}`,
      );
    }
    console.log(`signed as ${OWNER}: ${ownerKey.slice(0, 18)}`);
    break;
  }
  case "sign-granted": {
    expectProduct(GRANTED);
    await expectGrantedSignature();
    await checkEverySurface(false);
    break;
  }
  case "sign-again": {
    expectProduct(GRANTED);
    await expectGrantedSignature();
    break;
  }
  case "sign-untrusted": {
    expectProduct(UNTRUSTED);
    await expectRefused();
    await checkEverySurface(true);
    break;
  }
  default:
    throw new Error(
      `set E2E_PHASE to one of sign-owner, sign-granted, sign-untrusted, sign-again (got ${phase ?? "nothing"})`,
    );
}
