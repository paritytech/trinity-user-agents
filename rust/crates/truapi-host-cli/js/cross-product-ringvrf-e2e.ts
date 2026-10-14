// Cross-product ring-VRF signing against a real signing-host CLI.
//
// The sibling of `cross-product-storage-e2e.ts`, for the `context` scope. One
// product signs with another product's registered ring-VRF key, which the
// owner's manifest grant is the only thing permitting. The host resolves that
// grant from the `trustedProducts` in a local product config, so the flow runs
// before either product is deployed. See `--product-config` and
// `truapi-host-cli/src/product_config.rs`.
//
// This is the first end-to-end exercise of a *granted* cross-product call.
// Every other run of this path asserts the refusal: the generated
// `account-create-account-proof` example, and `ring-vrf-e2e.ts`, both pin
// `NotAllowlisted`. Neither can see a grant being honoured, which is how the
// scope shipped inert.
//
// Unlike the storage sibling this does touch a chain: registering a ring-VRF
// key resolves a ring on the People chain.
//
// The runner serves one product per host process, so
// `scripts/cross-product-ringvrf-e2e.sh` invokes this once per phase with
// `E2E_PHASE` set, pointing every run at the same `--base-path` so the key
// registered in one is there for the next.
//
// Phases, and what each proves:
//
//   register        peopl.paseo registers a ring-VRF key and signs with it,
//                   then stores the signature in its own storage.
//   sign-granted    dim2.paseo signs with peopl.paseo's key handle. Named in
//                   trustedProducts with `context`, so allowed, and the
//                   signature must equal the owner's own, which is what proves
//                   the grant reached the owner's key rather than deriving a
//                   new one for the caller.
//   sign-untrusted  stash.paseo signs the same handle. Same target, absent
//                   from trustedProducts: refused.
//   sign-again      dim2.paseo signs once more, so a refusal above cannot be
//                   the registration having gone.

import { PASEO_NEXT_V2_INDIVIDUALITY } from "../../../../js/packages/truapi/src/index.ts";

const OWNER = "peopl.paseo";
const GRANTED = "dim2.paseo";
const UNTRUSTED = "stash.paseo";

/// "pop:polkadot.network/people-lite", hex.
const PEOPLE_LITE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";

const RING = {
  chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
  junctions: [
    { tag: "CollectionId" as const, value: PEOPLE_LITE_COLLECTION_ID },
  ],
};

const INDEX = { tag: "Index" as const, value: 0 };
const OWNER_HANDLE = { dotNsIdentifier: OWNER, derivationIndex: INDEX };
/// "granted", as the hex the message takes.
const MESSAGE = "0x6772616e746564";
/// Where the register phase parks the owner's own signature for comparison.
const SIGNATURE_KEY = "owner-signature";

const REFUSAL = "NotAllowlisted";

function stringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/// The refusal variant, or `null` for anything else.
function refusalTag(error: unknown): string | null {
  const domain = error as {
    tag?: string;
    value?: { value?: { tag?: string } };
  };
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

async function signOwnerKey(): Promise<{
  ok: boolean;
  value?: string;
  error?: unknown;
}> {
  const signed = await truapi.account.ringVrfSign({
    keyHandle: OWNER_HANDLE,
    message: MESSAGE,
  });
  return signed.isOk()
    ? { ok: true, value: signed.value as unknown as string }
    : { ok: false, error: signed.error };
}

/// Sign `OWNER`'s key and require the signature the owner itself produced.
async function expectGrantedSignature(): Promise<void> {
  const signed = await signOwnerKey();
  if (!signed.ok) {
    throw new Error(
      `${host.productId} was granted ${OWNER} context but refused: ${stringify(signed.error)}`,
    );
  }
  const signature = signed.value ?? "";
  if (signature.length !== 2 + 64 * 2) {
    throw new Error(
      `expected a 64-byte signature, got ${signature.length} chars: ${signature}`,
    );
  }

  // Read what the owner signed, through the storage grant, and require the
  // same bytes. A host deriving the key from the caller rather than the handle
  // owner still returns a valid 64-byte signature, and only the comparison
  // catches it.
  const stored = await truapi.localStorage.read({
    product: OWNER,
    key: SIGNATURE_KEY,
  });
  if (!stored.isOk() || !stored.value.value) {
    throw new Error(
      `could not read ${OWNER}'s own signature back: ${stringify(stored.isOk() ? stored.value : stored.error)}`,
    );
  }
  if (signature !== stored.value.value) {
    throw new Error(
      `granted signature is not the owner's own: got ${signature}, owner produced ${stored.value.value}`,
    );
  }
  console.log(
    `granted ${host.productId} -> ${OWNER}: owner's own signature, ${signature}`,
  );
}

/// Sign `OWNER`'s key and require the standard refusal.
async function expectRefused(): Promise<void> {
  const signed = await signOwnerKey();
  if (signed.ok) {
    throw new Error(
      `${host.productId} signed with ${OWNER}'s key without a grant: ${signed.value}`,
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
  case "register": {
    expectProduct(OWNER);
    const registered = await truapi.account.registerRingVrfKey({
      index: INDEX,
      ring: RING,
    });
    if (!registered.isOk()) {
      throw new Error(
        `register_ring_vrf_key failed: ${stringify(registered.error)}`,
      );
    }
    const signed = await signOwnerKey();
    if (!signed.ok) {
      throw new Error(
        `the owner could not sign with its own key: ${stringify(signed.error)}`,
      );
    }
    const written = await truapi.localStorage.write({
      key: SIGNATURE_KEY,
      value: signed.value as string,
    });
    if (!written.isOk()) {
      throw new Error(
        `could not park the owner signature: ${stringify(written.error)}`,
      );
    }
    console.log(`registered and signed as ${OWNER}: ${signed.value}`);
    break;
  }
  case "sign-granted":
  case "sign-again": {
    expectProduct(GRANTED);
    await expectGrantedSignature();
    break;
  }
  case "sign-untrusted": {
    expectProduct(UNTRUSTED);
    await expectRefused();
    break;
  }
  default:
    throw new Error(
      `set E2E_PHASE to one of register, sign-granted, sign-untrusted, sign-again (got ${phase ?? "nothing"})`,
    );
}

export {};
