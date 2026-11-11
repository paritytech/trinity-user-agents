/**
 * The stacks this host can run on, and what each needs to reach its chains
 * and read DotNS names on it.
 *
 * A stack is chosen once, before boot, and holds for the life of the page:
 * the host never switches stack while it runs. Its chains, genesis hashes,
 * RPC nodes, TLD, DotNS resolver and services come from the stack's Trinity
 * stack descriptor, read by `@parity/trinity-stack`.
 *
 * - A built-in stack is one this build ships: its link pins the key every
 *   copy must be signed with, and the build bundles a signed copy from
 *   `rust/crates/truapi-host-cli/stacks/`, which `scripts/update-stacks.sh`
 *   refreshes.
 * - A custom stack is one the user added by its URL or stack link
 *   (`stacks.ts`). Only a link with a key pins a signer; a plain URL is read
 *   unsigned, trusted as far as whoever serves it.
 *
 * Everything a descriptor carries is read from it: the menu name, the chains,
 * RPC nodes, TLD, the network `getChainInfo` reports, the DotNS resolver and
 * the services. What stays here is what a descriptor does not carry: each
 * built-in's pinned link and bundled copy, the namespace its state is kept
 * under and its best-block posting candidate; and, for every stack, which chain specs the provider
 * bundles and the resolver's storage slots.
 */

import { sha256 } from "@noble/hashes/sha2.js";
import {
  decodeLink,
  open,
  stackId,
  type Descriptor,
  type Pointer,
} from "@parity/trinity-stack";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import paseoNextV2Copy from "../../../../rust/crates/truapi-host-cli/stacks/paseo-next-v2.json";
import pcfDevnetCopy from "../../../../rust/crates/truapi-host-cli/stacks/pcf-devnet.json";
import previewnetCopy from "../../../../rust/crates/truapi-host-cli/stacks/previewnet.json";
import { DEFAULT_NETWORK as DEFAULT_NETWORK_ID } from "./network-scope.js";

/** Id of a stack this build ships. Each is also the namespace its state is kept under. */
export type NetworkId = "paseo" | "previewnet" | "pcf-devnet";

/** A chain of a stack that this host connects to. */
export type ChainRole = "relay" | "assetHub" | "people" | "bulletin";

/** Every chain a stack has, in the order the host connects them. */
export const CHAIN_ROLES: readonly ChainRole[] = [
  "relay",
  "assetHub",
  "people",
  "bulletin",
];

/** The role a descriptor lists each chain under. */
const DESCRIPTOR_ROLES: Record<ChainRole, string> = {
  relay: "Relay",
  assetHub: "AssetHub",
  people: "People",
  bulletin: "Bulletin",
};

/** The TLDs the core derives identities under (`DOTNS_TLDS` in the core). */
export const DOTNS_TLDS: readonly string[] = ["dot", "paseo", "testnet"];

/**
 * The networks whose chain specs `truapi-provider` bundles, by the id of the
 * stack whose chains they are: the light client can start only from these.
 */
const CATALOG_NETWORKS: readonly string[] = ["paseo-next-v2", "previewnet"];

/**
 * How the host reaches one chain: through the chain's RPC node, which it
 * trusts, or through the embedded light client, which checks what it reads.
 */
export type ChainBackend = "rpc" | "light-client";

/** One chain's RPC node and the backend the host uses for it unless told otherwise. */
export interface ChainAccess {
  /** The chain's RPC node (`wss`): the first the stack's descriptor lists. */
  rpc: string;
  backend: ChainBackend;
}

/**
 * The backend each chain of `config` is reached through: its default, unless
 * `overrides` names another.
 */
export function chainBackends(
  config: NetworkConfig,
  overrides: Partial<Record<ChainRole, ChainBackend>> = {},
): Record<ChainRole, ChainBackend> {
  return Object.fromEntries(
    CHAIN_ROLES.map((role) => [
      role,
      overrides[role] ?? config.chains[role].backend,
    ]),
  ) as Record<ChainRole, ChainBackend>;
}

/** Where this host reads a stack's DotNS records and fetches product content. */
export interface DotnsEndpoints {
  /**
   * The `DotnsContentResolver` contract on Asset Hub, as a bare lower-case
   * H160 without `0x`, from the descriptor's `dotns.contentResolver`.
   */
  contentResolver: string;
  /** Slot of the `contenthash` mapping in that contract. */
  contenthashSlot: number;
  /**
   * Slot of the `text` mapping in that contract, `node => key => value`,
   * which holds a name's text records such as `executable`.
   */
  textSlot: number;
  /**
   * The origin of the stack's `ipfs-gateway`. The host appends the
   * `/ipfs/<cid>/` path to it, so a gateway is accepted only when it serves
   * at `/ipfs/` on its origin.
   */
  contentGateway: string;
}

/** A stack's descriptor as accepted, kept with the bytes it came in. */
export interface StackCopy {
  /** The stack's link: the descriptor's URL, and the key it verifies under when it has one. */
  pointer: Pointer;
  /**
   * The document as served or bundled. The core verifies these exact bytes
   * once it takes its chains from a stack.
   */
  document: Uint8Array;
  descriptor: Descriptor;
}

/** What the host decides about a stack, which its descriptor does not say. */
export interface StackPolicy {
  /** The namespace the stack's state is kept under, and the tab's choice. */
  id: string;
  /** Shipped by this build, or added by the user. */
  origin: "built-in" | "custom";
  /** See {@link NetworkConfig.bestBlockPosting}. */
  bestBlockPosting: { peopleGenesis: string } | null;
}

/** How this host reaches one stack's chains and reads its DotNS names. */
export interface NetworkConfig {
  /** The namespace the stack's state is kept under, and the tab's choice. */
  id: string;
  /** Human name used in messages: the descriptor's `name`, e.g. `Paseo Next v2`. */
  displayName: string;
  origin: "built-in" | "custom";
  /**
   * The network's name in the bundled `truapi-provider` catalog, which the
   * light client starts from: the descriptor's `id`, for a built-in stack the
   * catalog holds. Null for any other stack, every chain on its RPC node.
   */
  catalogNetwork: string | null;
  /** The network `getChainInfo` reports to products: the descriptor's `network`. */
  chainInfoNetwork: string;
  /**
   * The stack's dotNS TLD and the identity suffix the core derives product
   * accounts under, e.g. `paseo`, `testnet`.
   */
  networkSuffix: string;
  /**
   * The host name of the stack's `web-gateway`, e.g. `paseo.li`. It serves
   * the Polkadot browser shell and refuses framing, so it is never opened as
   * a product.
   */
  webGateway: string;
  /**
   * Each chain's RPC node and default backend. DotNS reads always go to the
   * Asset Hub node, over the `https` form of its URL.
   */
  chains: Record<ChainRole, ChainAccess>;
  /** Each chain's genesis hash, from the descriptor. */
  genesis: Record<ChainRole, HexString>;
  /** Read-only DotNS and content endpoints on this stack. */
  dotns: DotnsEndpoints;
  /**
   * The base URL of the stack's identity backend (`dub` service, ending in
   * `/`), which issues TURN relay credentials for a personhood proof and
   * answers username searches. Null when the stack names none.
   */
  identityBackend: string | null;
  /**
   * The People chain whose Statement Store accounts may post once their
   * claim's block has a child at the best block, rather than once their
   * allowance holds at the finalized head (`allowance-usable.ts`). Null: they
   * wait for the finalized head.
   *
   * Set only for a People chain whose RPC nodes were checked to keep a
   * statement whose allowance is not final yet (polkadot-sdk #13437). The
   * genesis only permits a candidate. Each best-block read must also verify
   * the actual RPC connection's genesis, version and required methods.
   */
  bestBlockPosting: { peopleGenesis: string } | null;
  /**
   * Whether the wallet's backup runs here. Off on a custom stack: its keys,
   * topic and channel derive from the wallet and the People genesis alone, so
   * a stack sharing a People chain with another would compete for one pointer.
   */
  backup: boolean;
  /** The descriptor every other field was read from. */
  stack: StackCopy;
}

/** A stack this build ships. */
export interface BuiltInStack extends StackPolicy {
  id: NetworkId;
  origin: "built-in";
  /**
   * The stack's link: where its descriptor is published and the key it must
   * be signed with. The key is what this build trusts; a copy signed by any
   * other key is refused.
   */
  pointer: string;
  /** The signed copy this build bundles, used until a newer one is accepted. */
  bundled: Uint8Array;
}

/**
 * A bundled copy's bytes. The JSON import parses the envelope, and the
 * envelope is written compactly, so serialising it again gives the file's
 * bytes; a unit test holds that.
 */
const bundledBytes = (envelope: object): Uint8Array =>
  new TextEncoder().encode(JSON.stringify(envelope));

/** RPC for the parachains, as on the native hosts; the light client for the relay. */
const DEFAULT_BACKENDS: Record<ChainRole, ChainBackend> = {
  relay: "light-client",
  assetHub: "rpc",
  people: "rpc",
  bulletin: "rpc",
};

/** Every chain on its RPC node, for a stack the provider's catalog does not hold. */
const RPC_BACKENDS: Record<ChainRole, ChainBackend> = {
  relay: "rpc",
  assetHub: "rpc",
  people: "rpc",
  bulletin: "rpc",
};

/** Every stack this build ships, by id, in the order the menu offers them. */
export const BUILT_IN_STACKS: Record<NetworkId, BuiltInStack> = {
  paseo: {
    id: "paseo",
    origin: "built-in",
    pointer:
      "polkadotapp://stack?key=0x2b89d8ca74aa2e11ab55f5af1168cd6929b88d1998aecd979b42cfc1078d5377&url=https%3A%2F%2Fstack-paseo-next.parity-testnet.parity.io%2F.well-known%2Ftrinity-stack.json",
    bundled: bundledBytes(paseoNextV2Copy),
    // This genesis is a candidate; the connected node must pass the RPC identity check.
    bestBlockPosting: {
      peopleGenesis:
        "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
    },
  },
  previewnet: {
    id: "previewnet",
    origin: "built-in",
    pointer:
      "polkadotapp://stack?key=0x4d43362f03d5511632a210632d1fb7760b5a663580f12ffca34619f79357e239&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json",
    bundled: bundledBytes(previewnetCopy),
    // Its People node (1.24.3, 2026-09-16) predates #13437.
    bestBlockPosting: null,
  },
  "pcf-devnet": {
    id: "pcf-devnet",
    origin: "built-in",
    pointer:
      "polkadotapp://stack?key=0x1fbc5eed0deb09fe4eeefdf45350b4b4cf36370d9bbb93ca2818b076737a0ed3&url=https%3A%2F%2Fstack.api.polkadotcommunity.foundation%2F.well-known%2Ftrinity-stack.json",
    bundled: bundledBytes(pcfDevnetCopy),
    bestBlockPosting: null,
  },
};

/** A descriptor this host cannot run on, and the rule it breaks. */
export class StackRefusedError extends Error {
  override name = "StackRefusedError";
}

/** The bundled copy of `builtIn`, checked against its link's key. */
export function bundledCopy(builtIn: BuiltInStack): StackCopy {
  const pointer = decodeLink(builtIn.pointer);
  const { descriptor } = open(builtIn.bundled, pointer);
  return { pointer, document: builtIn.bundled, descriptor };
}

/**
 * The namespace a custom stack's state is kept under: a hash of its identity,
 * which is its key when the link carries one and its URL otherwise. Never the
 * descriptor's `id`, which any operator can set.
 */
export function customStackId(pointer: Pointer): string {
  const digest = sha256(new TextEncoder().encode(stackId(pointer)));
  return `custom-${bytesToHex(digest).slice(2, 18)}`;
}

/** The host's policy for the custom stack at `pointer`. */
export function customPolicy(pointer: Pointer): StackPolicy {
  return {
    id: customStackId(pointer),
    origin: "custom",
    bestBlockPosting: null,
  };
}

/** The menu name of a stack whose descriptor is named `name`. */
export const stackName = (policy: StackPolicy, name: string): string =>
  policy.origin === "custom" ? `${name} (custom)` : name;

/** The first URL of the service of `kind`, or null when the stack names none. */
function serviceUrl(descriptor: Descriptor, kind: string): string | null {
  return (
    descriptor.services.find((service) => service.kind === kind)?.urls[0] ??
    null
  );
}

/**
 * The config of the stack `policy` describes, read from `copy`. Throws
 * {@link StackRefusedError} when the descriptor lacks what this host needs:
 * the Relay, Asset Hub, People and Bulletin chains with distinct genesis
 * hashes, a TLD the core derives identities under, a DotNS content resolver,
 * an IPFS gateway serving at `/ipfs/` and a web gateway. Any stack may reuse
 * another's TLD or chains, Polkadot's included. A missing identity backend
 * only turns off what needs it.
 */
export function networkConfigFrom(
  policy: StackPolicy,
  copy: StackCopy,
): NetworkConfig {
  const { descriptor } = copy;
  const displayName = stackName(policy, descriptor.name);
  const refuse = (rule: string): never => {
    throw new StackRefusedError(
      `The ${displayName} stack descriptor (sequence ${descriptor.sequence}) ${rule}.`,
    );
  };
  const chainOf = (role: ChainRole) =>
    descriptor.chains.find((chain) => chain.role === DESCRIPTOR_ROLES[role]) ??
    refuse(`has no ${DESCRIPTOR_ROLES[role]} chain`);
  const chains = Object.fromEntries(
    CHAIN_ROLES.map((role) => [role, chainOf(role)]),
  ) as Record<ChainRole, ReturnType<typeof chainOf>>;
  const genesis = Object.fromEntries(
    CHAIN_ROLES.map((role) => [role, chains[role].genesisHash as HexString]),
  ) as Record<ChainRole, HexString>;
  // The provider keeps one node per genesis hash, so two roles on one hash would share the last one.
  if (new Set(Object.values(genesis)).size !== CHAIN_ROLES.length)
    refuse("gives two chains one genesis hash");

  if (!DOTNS_TLDS.includes(descriptor.tld))
    refuse(
      `uses the TLD ${descriptor.tld}; this host's core derives identities only under ${DOTNS_TLDS.join(", ")}`,
    );

  const contentResolver = descriptor.dotns?.contentResolver;
  if (contentResolver === undefined) refuse("names no DotNS content resolver");

  const ipfsGateway = serviceUrl(descriptor, "ipfs-gateway");
  if (ipfsGateway === null) refuse("names no IPFS gateway");
  const gateway = new URL(ipfsGateway!);
  if (gateway.pathname !== "/ipfs/" || gateway.search !== "")
    refuse(
      `has an IPFS gateway that does not serve at /ipfs/ (${ipfsGateway})`,
    );

  const webGateway = serviceUrl(descriptor, "web-gateway");
  if (webGateway === null) refuse("names no web gateway");

  // A custom stack never starts from the catalog: a fork keeps its source's
  // genesis hashes, and the catalog's chain specs would sync the source.
  const catalogNetwork =
    policy.origin === "built-in" && CATALOG_NETWORKS.includes(descriptor.id)
      ? descriptor.id
      : null;
  const backends = catalogNetwork === null ? RPC_BACKENDS : DEFAULT_BACKENDS;
  return {
    id: policy.id,
    displayName,
    origin: policy.origin,
    catalogNetwork,
    chainInfoNetwork: descriptor.network,
    networkSuffix: descriptor.tld,
    webGateway: new URL(webGateway!).hostname,
    chains: Object.fromEntries(
      CHAIN_ROLES.map((role) => [
        role,
        { rpc: chains[role].rpc[0], backend: backends[role] },
      ]),
    ) as Record<ChainRole, ChainAccess>,
    genesis,
    dotns: {
      contentResolver: contentResolver!.replace(/^0x/, "").toLowerCase(),
      contenthashSlot: 0,
      textSlot: 1,
      contentGateway: gateway.origin,
    },
    identityBackend: serviceUrl(descriptor, "dub"),
    bestBlockPosting: policy.bestBlockPosting,
    backup: policy.origin === "built-in",
    stack: copy,
  };
}

/**
 * Every stack this build ships, by id, as its bundled copy describes it. A
 * tab runs on the copy `stacks.ts` accepts, which can be a newer one;
 * build-time tools and the dev server read these.
 */
export const NETWORKS: Record<NetworkId, NetworkConfig> = Object.fromEntries(
  Object.values(BUILT_IN_STACKS).map((builtIn) => [
    builtIn.id,
    networkConfigFrom(builtIn, bundledCopy(builtIn)),
  ]),
) as Record<NetworkId, NetworkConfig>;

/**
 * The default stack's config, for callers that have not chosen one.
 *
 * The default id itself is `DEFAULT_NETWORK` in `network-scope.ts`, shared
 * with the per-tab selection and the storage keys, so there is one answer to
 * which stack a tab runs on.
 */
export const DEFAULT_NETWORK_CONFIG: NetworkConfig = networkConfig();

/** Whether `id` names a stack this build ships. */
export function isNetworkId(id: string): id is NetworkId {
  return Object.hasOwn(BUILT_IN_STACKS, id);
}

/** The id `id` names, or the default when it is empty or absent. Throws for an unknown id. */
function networkId(id?: string | null): NetworkId {
  const trimmed = id?.trim() ?? "";
  const chosen = trimmed === "" ? DEFAULT_NETWORK_ID : trimmed;
  if (isNetworkId(chosen)) return chosen;
  throw new Error(
    `Unknown network ${JSON.stringify(id)}. Known networks: ${Object.keys(NETWORKS).join(", ")}.`,
  );
}

/**
 * The bundled config for `id`, or the default when `id` is empty or absent.
 * Throws for an unknown id rather than silently serving the default, so a
 * typo in the selection is reported instead of connecting to the wrong
 * stack.
 */
export function networkConfig(id?: string | null): NetworkConfig {
  return NETWORKS[networkId(id)];
}

/** The built-in stack for `id`, with the same defaulting and refusal as {@link networkConfig}. */
export function builtInStack(id?: string | null): BuiltInStack {
  return BUILT_IN_STACKS[networkId(id)];
}
