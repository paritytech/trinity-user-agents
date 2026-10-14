// Cross-product storage against a real signing-host CLI.
//
// One product writes to its own storage and another reads it, which the manifest
// grant is the only thing permitting. The host resolves that grant from the
// `trustedProducts` in a local product config, which seeds the manifest cache,
// so the granted read runs before either product is deployed and without a
// chain. See `--product-config` and `truapi-host-cli/src/product_config.rs`.
//
// A target with no seeded entry is not chain-free: the cache miss goes to dotNS
// on Asset Hub before refusing, so the `read-missing` phase does reach the
// network.
//
// The runner serves one product per host process, so `scripts/cross-product-storage-e2e.sh`
// invokes this once per phase with `E2E_PHASE` set, pointing every run at the
// same `--base-path` so the storage written in one is there for the next.
//
// Phases, and what each proves:
//
//   write            peopl.paseo stores a value in its own storage.
//   read             dim2.paseo reads it. Named in trustedProducts, so allowed.
//   read-untrusted   stash.paseo reads the same key. Same target, same value,
//                    absent from trustedProducts: refused.
//   read-missing     dim2.paseo reads a product that published nothing. Must be
//                    refused identically to the above, or the call becomes a
//                    probe for which products exist.
//   read-again       dim2.paseo reads once more, so a refusal above cannot be
//                    the value having expired or gone.

const OWNER = "peopl.paseo";
const GRANTED = "dim2.paseo";
const UNTRUSTED = "stash.paseo";
const NO_MANIFEST = "nobody.paseo";
const KEY = "unwrapped";
/// "state", as the hex the storage codec takes.
const VALUE = "0x7374617465";

const REFUSAL = "AccessNotGranted";

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

/// Read `product`'s storage and require the standard refusal.
async function expectRefused(product: string): Promise<void> {
  const read = await truapi.localStorage.read({ product, key: KEY });
  if (read.isOk()) {
    throw new Error(
      `${host.productId} read ${product} without a grant: ${stringify(read.value)}`,
    );
  }
  const tag = refusalTag(read.error);
  if (tag !== REFUSAL) {
    // Anything else leaks why it failed, which is what one refusal prevents.
    throw new Error(
      `${host.productId} -> ${product}: expected ${REFUSAL}, got ${stringify(read.error)}`,
    );
  }
  console.log(`refused ${host.productId} -> ${product}: ${REFUSAL}`);
}

/// Read `OWNER`'s storage and require the value written in the write phase.
async function expectRead(): Promise<void> {
  const read = await truapi.localStorage.read({ product: OWNER, key: KEY });
  if (!read.isOk()) {
    throw new Error(
      `${host.productId} was granted ${OWNER} but refused: ${stringify(read.error)}`,
    );
  }
  const value = read.value.value;
  if (!value) {
    // The grant resolved and the host answered empty, which is what a host
    // keying storage off the caller rather than the owner in the key does.
    throw new Error(
      `${host.productId} read ${OWNER} and got nothing: the host keyed storage off the caller`,
    );
  }
  if (value !== VALUE) {
    throw new Error(`expected ${VALUE} from ${OWNER}, got ${value}`);
  }
  console.log(`read ${host.productId} -> ${OWNER}: ${value}`);
}

const phase = process.env.E2E_PHASE;

switch (phase) {
  case "write": {
    expectProduct(OWNER);
    const written = await truapi.localStorage.write({ key: KEY, value: VALUE });
    if (!written.isOk()) {
      throw new Error(`${OWNER} could not write its own storage: ${stringify(written.error)}`);
    }
    console.log(`wrote ${OWNER}/${KEY}`);
    break;
  }
  case "read":
  case "read-again": {
    expectProduct(GRANTED);
    await expectRead();
    break;
  }
  case "read-untrusted": {
    expectProduct(UNTRUSTED);
    await expectRefused(OWNER);
    break;
  }
  case "read-missing": {
    expectProduct(GRANTED);
    await expectRefused(NO_MANIFEST);
    break;
  }
  default:
    throw new Error(`set E2E_PHASE to one of write, read, read-untrusted, read-missing, read-again (got ${phase ?? "nothing"})`);
}
