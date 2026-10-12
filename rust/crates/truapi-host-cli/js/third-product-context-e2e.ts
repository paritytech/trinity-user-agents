// A ring-VRF proof in a third product's context, against a real signing-host
// CLI. The end-to-end check for paritytech/platform-bugs#99.
//
// dim2next.paseo holds a grant to peopl.paseo's ring-VRF key. That grant lets it
// use the key; it does not let it choose whose pseudonym to mint. A proof in a
// third product's context needs that product's consent too, through `context`
// in its own manifest. The grants come from local product configs, see
// `--product-config` and `truapi-host-cli/src/product_config.rs`:
//
//   peopl.paseo   grants dim2next `context`: the key grant.
//   dim2.paseo    grants dim2next `context`: the third product consents.
//   stash.paseo   grants nothing.
//
// The runner serves one product per host process, so
// `scripts/third-product-context-e2e.sh` invokes this once per phase with
// `E2E_PHASE` set, pointing every run at the same `--base-path` so the key
// registered in one is there for the next.
//
// Phases, and what each proves:
//
//   register  peopl.paseo registers a ring-VRF key.
//   prove     dim2next.paseo proves with peopl.paseo's key, and reads its
//             alias, in dim2.paseo's context (admitted) and in stash.paseo's
//             (refused).
//
// Reading the result: an admitted proof still comes back `NotMember`. The
// context is checked first, and the CLI account is not a person, so the proof
// fails one check later. The refusal is `NotAllowlisted` for a proof and
// `Rejected` for an alias. Before the fix both contexts were refused.

const OWNER = "peopl.paseo";
const CALLER = "dim2next.paseo";
const GRANTED = "dim2.paseo";
const UNGRANTED = "stash.paseo";

/// "pop:polkadot.network/people-lite", hex.
const PEOPLE_LITE_COLLECTION_ID =
  "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";

const INDEX = { tag: "Index" as const, value: 0 };
const OWNER_HANDLE = { dotNsIdentifier: OWNER, derivationIndex: INDEX };
/// "turn", as the hex the message takes.
const MESSAGE = "0x7475726e";

/// The refusal each call answers. The alias error has no `NotAllowlisted`, so
/// the host maps the same refusal to `Rejected` there.
const PROOF_REFUSAL = "NotAllowlisted";
const ALIAS_REFUSAL = "Rejected";
/// What an admitted proof answers when the account is not a person.
const ADMITTED_NOT_MEMBER = "NotMember";

function stringify(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/// The domain error variant, or `null` for anything else.
function domainTag(error: unknown): string | null {
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

async function ringLocation() {
  const people = await truapi.chain.getChainInfo({ chain: "People" });
  if (!people.isOk()) {
    throw new Error(`get_chain_info failed: ${stringify(people.error)}`);
  }
  return {
    chainId: people.value.genesisHash,
    junctions: [
      { tag: "CollectionId" as const, value: PEOPLE_LITE_COLLECTION_ID },
    ],
  };
}

type Ring = Awaited<ReturnType<typeof ringLocation>>;

/// `Ok`, or the domain error variant.
async function prove(ring: Ring, productId: string): Promise<string> {
  const proof = await truapi.account.createAccountProof({
    keyHandle: OWNER_HANDLE,
    context: { productId, suffix: INDEX },
    ringLocation: ring,
    message: MESSAGE,
  });
  return proof.isOk()
    ? "Ok"
    : (domainTag(proof.error) ?? stringify(proof.error));
}

/// `Ok`, or the domain error variant.
async function readAlias(ring: Ring, productId: string): Promise<string> {
  const alias = await truapi.account.getAccountAlias({
    keyHandle: OWNER_HANDLE,
    context: { productId, suffix: INDEX },
    ringLocation: ring,
  });
  return alias.isOk()
    ? "Ok"
    : (domainTag(alias.error) ?? stringify(alias.error));
}

/// Admitted means past the context check: `Ok`, or `NotMember` one check later.
function expectAdmitted(call: string, productId: string, result: string): void {
  if (result !== "Ok" && result !== ADMITTED_NOT_MEMBER) {
    throw new Error(
      `${call} in ${productId}: ${productId} grants ${CALLER} context, but got ${result}`,
    );
  }
  const note =
    result === ADMITTED_NOT_MEMBER ? " (the CLI account is not a person)" : "";
  console.log(`admitted ${call} in ${productId}: ${result}${note}`);
}

function expectRefused(
  call: string,
  productId: string,
  result: string,
  refusal: string,
): void {
  if (result !== refusal) {
    throw new Error(
      `${call} in ${productId}: ${productId} grants ${CALLER} nothing, expected ${refusal}, got ${result}`,
    );
  }
  console.log(`refused ${call} in ${productId}: ${result}`);
}

const phase = process.env.E2E_PHASE;

switch (phase) {
  case "register": {
    expectProduct(OWNER);
    const registered = await truapi.account.registerRingVrfKey({
      index: INDEX,
      ring: await ringLocation(),
    });
    if (!registered.isOk()) {
      throw new Error(
        `register_ring_vrf_key failed: ${stringify(registered.error)}`,
      );
    }
    console.log(`registered ${OWNER}'s ring-VRF key`);
    break;
  }
  case "prove": {
    expectProduct(CALLER);
    const ring = await ringLocation();
    expectAdmitted("proof", GRANTED, await prove(ring, GRANTED));
    expectRefused(
      "proof",
      UNGRANTED,
      await prove(ring, UNGRANTED),
      PROOF_REFUSAL,
    );
    expectAdmitted("alias", GRANTED, await readAlias(ring, GRANTED));
    expectRefused(
      "alias",
      UNGRANTED,
      await readAlias(ring, UNGRANTED),
      ALIAS_REFUSAL,
    );
    break;
  }
  default:
    throw new Error(
      `set E2E_PHASE to one of register, prove (got ${phase ?? "nothing"})`,
    );
}

export {};
