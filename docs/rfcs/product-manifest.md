---
title: "Product Manifest Format"
owner: "@johnthecat"
---

# RFC — Product Manifest Format

## Summary

A product's bytes live on the Bulletin chain; the manifest describes them. It is the metadata a Polkadot User Agent needs to present a product, know what it offers, and make sense of what it fetches:

- **Describe the product to the user** — `displayName`, `description`, and an icon: enough to list and recognise a product before anything runs.
- **Declare what can be interacted with** — which surfaces the product serves, and which other products may interact with it without a consent prompt.
- **Interpret the fetched bytes** — which `kind` of executable an archive holds, its `entrypoint`, the sizes a widget renders at. Without these the archive is an undifferentiated directory of files.

The format is two-level:

- The **root manifest** carries product-wide metadata (displayName, icon, description) and the product's standing trust grants to other products, and lives at the product's dotNS base name. Authorship is read from dotNS itself (the on-chain owner of the name) rather than declared in the manifest.
- One or more **executable manifests** describe individual executables (App, Widget, Worker). Each pins a product-defined version and lives at a well-known subname of the base name (`app.<product_id>.<tld>`, `widget.<product_id>.<tld>`, `worker.<product_id>.<tld>`).

Manifests are JSON, stored inline in dotNS text records. Binary content lives on the Bulletin chain.

## Motivation

### Background

Polkadot User Agents, or **Hosts** (Polkadot Desktop, Polkadot Mobile, the Polkadot Website), render third-party applications called **Products**.

A product can expose one or more **modalities**, each a distinct user-facing surface:

- **App** — full-screen web application.
- **Widget** — small web application mounted on a dashboard.
- **Pocket** — passive surfaces such as cards, tickets, or certificates, served by a background JS worker.
- **Chat** — chat bots and chat-room integrations, served by a background JS worker.
- **Input** — an input surface for contextual interactions with the Products, served by a background JS worker.

A modality is delivered by an **executable**. v1 defines three executable types:

- **App** — the web application backing the App modality.
- **Widget** — the web application backing the Widget modality.
- **Worker** — a single background process. It may back any combination of Pocket, Chat, and Input, or serve no user-facing surface at all and run purely as background logic (see [Why one Worker, not per modality](#executable-manifest-v1)).

Throughout this RFC, *modality* means a user-facing surface; *executable* means a deployable artifact.

Two on-chain systems sit under this RFC:

- **Bulletin chain** — stores binary blobs. Each blob is addressed by a content identifier ("CID") derived from its bytes. Executable artifacts and product icons live there.
- **dotNS** — Polkadot's on-chain naming system, implemented as contracts on Asset Hub. A name owns text records that hold small structured metadata. Manifests live there.

### Requirements

For a Host to discover, verify, and launch a product, it needs a static, on-chain, authenticated description binding a dotNS name to a specific executable revision. Without a standardized manifest, every Host invents its own discovery convention. The manifest format must therefore:

1. Provide static product-wide metadata: displayName, icon, and description.
2. Let a product declare which other products may interact with it without a per-call consent prompt.
3. Provide, for each executable, a version.
4. Be discoverable through a storage call or JSON-RPC call to a node.
5. Be encoded in a format any client environment can parse with off-the-shelf tooling.
6. Fit inside dotNS text records as a single inline payload.
7. Be versionable.

## Detailed Design

### Overview

A product is rooted at a **dotNS base name** (e.g. `game.dot`). The base name's text records carry the **root manifest**. Each executable is rooted at a well-known **subname** of the base name (e.g. `widget.game.dot`), whose text records carry the corresponding **executable manifest**.

**Terminology.** `<product_id>` is the label portion of the base name (`game` for `game.dot`). `<tld>` is the top-level label the dotNS deployment declares — `dot` on mainnet, `paseo` on Paseo. The same product is `game.dot` on one network and `game.paseo` on another, so manifests identify products by `<product_id>` alone and never pin a TLD.

```
game.dot                  → root manifest (displayName, icon, description)
app.game.dot              → executable manifest (App)
widget.game.dot           → executable manifest (Widget)
worker.game.dot           → executable manifest (Worker; serves Pocket, Chat and/or Input)
```

A Host discovers a product's executables by querying these subnames. Absence of a subname means the product does not provide that executable.

### Encoding and storage

Manifests are encoded as **UTF-8 JSON** and stored **inline** in a single, well-known text record key on the (sub)name: the value of that text record is the manifest JSON itself. The only binary reference inside a manifest is the icon's short CID string; no binary payloads are inlined.

The records are fixed by this RFC:

| Subject                            | Record                   |
|------------------------------------|--------------------------|
| Root manifest (on the base name)   | text record `manifest`   |
| Executable manifest (on a subname) | text record `executable` |
| Executable bytes (on a subname)    | `contenthash`            |

Hosts MUST read exactly these records; publishers MUST write exactly these records. Keeping them distinct means a wrong-layer query (e.g. `manifest` on `app.<product_id>.<tld>`) returns an empty value instead of partially parsing a payload of the wrong shape.

The split between the last two rows is deliberate: the manifest *describes* an executable, the `contenthash` *locates* it. No CID for executable bytes appears inside the JSON. Hosts MUST read an IPFS-codec `contenthash`; other codecs are not part of v1.

A v1 root manifest carrying no trust grants is well under 1 KB; an executable manifest is ~200 B. `trustedProducts` is the only unbounded field in v1 and counts against the same budget. Manifests fit within typical text-record budgets; the exact figure will be confirmed by the dotNS team's Proof-of-Concept (see [Unresolved Questions](#unresolved-questions)). v1 defines no preimage fallback: a manifest that cannot fit MUST be shrunk by the publisher.

### Versioning

Every manifest carries `$v` as its first field — a numeric schema-version discriminator. This RFC defines `$v: 1`. Hosts MUST treat any manifest whose `$v` they do not recognise as an undiscoverable product: skip it, surface a diagnostic, and keep working.

### Root manifest (v1)

The root manifest describes the product as a whole and is the resolution entry point: a Host parses it first, then probes for executable subnames.

```typescript
type RootManifest = {
  $v: 1;
  displayName: string;    // Human-readable product name. UTF-8.
  description: string;    // Short description shown in launchers/lists.
  icon: Icon;             // Product icon used by every Host surface.
  trustedProducts?: Record<string, Granted[]>; // `<product_id>`, no TLD suffix → what that product may do to this one.
};

type Icon = {
  cid: string;            // Bulletin-chain CID; used verbatim to fetch icon bytes.
  format: 'jpeg' | 'png'; // Formats defined by v1. An unrecognised value is tolerated, not fatal.
};

type Granted =            // Grants v1 defines. Unrecognised values are ignored, not fatal.
  | 'all'                 // Wildcard: every mediated interaction, present and future.
  | 'storage'             // Read this product's host-local storage.
  | 'context';            // Act as this product's account: read it, the identity
                          // behind it, and sign under its keys.
```

#### Icons

The icon is always deployed on the Bulletin chain — there is no inline-icon variant in v1.

**The icon is presentational and never load-bearing.** Every icon failure resolves the same way: render a placeholder, keep the product launchable.
Two obligations follow. A validator MUST accept a `RootManifest` whose `format` is not one of the v1 values, and Hosts MUST NOT sniff or auto-correct the format.

#### Trusted products

Products interact through the Host — reading another product's account, asking it to sign.
Each such interaction is normally a consent decision; `trustedProducts` pre-approves the ones the publisher has already judged safe, and the Host does not prompt for them.

**The grant is issued by the product being accessed.** An entry in A's manifest states what B may do *to A* — the only direction A's name can authenticate. It says nothing about what A may do to B, nor about the products B in turn trusts.

- **Keys** are bare `<product_id>` labels, lowercase, with no TLD suffix: `"wallet"`, never `"wallet.dot"`. The Host appends the TLD of the network it resolves against. A key that does not resolve there is inert, not a validation error.
- **Values** are that product's grants. v1 defines three. `all` is a wildcard for the complete set of cross-product permissions the Host mediates on this product's behalf: it is resolved against that set when the grant is used, not enumerated here, so a grant of `all` covers permissions added after it was published. `storage` covers reading this product's host-local storage, read-only. `context` covers acting as this product's account: reading it and the identity that follows from it, and producing ring-VRF proofs and signatures under its keys. Granting it to another product lets that product sign as this one, so it is materially wider than a read. Hosts MUST ignore unrecognised values, keep the recognised ones, and MUST NOT fail validation over them.
- **`all` is a superset, not a peer.** `["all"]` implies `storage` and `context`, so `["all", "storage"]` is `["all"]` and a Host MUST NOT read a narrower value as a restriction on `all`. Enumerating the narrow values instead of granting `all` covers the same interactions today but does not widen when a further value is defined. Values are a set: order is not significant and duplicates collapse. Scopes are independent — a grant of `["storage"]` leaves account interactions prompting as usual.
- **Absence means no grants.** Missing field, empty record, and empty array are equivalent: prompt as usual. A product listing itself is ignored.

Which interactions a Host mediates, and what the prompt looks like, are Host runtime contracts; this RFC defines only how the grants are published and read.

### Executable manifest (v1)

An executable manifest describes one deployable artifact and lives on a well-known subname keyed by its executable type.

```typescript
type ExecutableManifest =
  | AppManifest
  | WidgetManifest
  | WorkerManifest;

type CommonExecutableFields = {
  $v: 1;
  appVersion: SemVer;     // Product-defined SemVer of this executable.
};

type AppManifest = CommonExecutableFields & {
  kind: 'app';
};

type WidgetManifest = CommonExecutableFields & {
  kind: 'widget';
  description?: string;          // Optional tagline shown on the widget card.
  dimensions: {
    height: number[];            // Supported grid-step heights the widget can render at.
    width?: number;              // Grid-step width. Optional; defaults to 1 column.
  };
};

type WorkerManifest = CommonExecutableFields & {
  kind: 'worker';
  entrypoint: string;                              // Path to the worker entry module inside the executable directory.
  includes: {                                      // Surfaces served; an omitted key means `false`.
    pocket?: boolean;
    chat?: boolean;
    input?: boolean;
  };
};

type SemVer = [major: number, minor: number, patch: number, build?: string];
// e.g. [1, 0, 0] or [1, 0, 0, '<build identifier, e.g. commit hash>']
```

- `app` — full-screen App. No extra fields beyond the common ones.
- `widget` — `dimensions.height` is the list of grid-step heights the widget can render at; the Host picks one per layout. `width` defaults to `1` column. The grid unit and bounds belong to the Host's dashboard spec (see [Future Directions](#future-directions)). By convention `8` in `height` signals a full-screen widget; this RFC does not normalise that convention.
- `worker` — background JS worker. `entrypoint` is the module the Host loads inside the worker. `includes` declares which surfaces it serves.

**`appVersion` is a label, not a change signal.** Hosts detect a new deployment from the subname's `contenthash`, not from this field (see [Cache invalidation](#resolving-a-product)). `appVersion` names the release for the user — "update to 1.4.0", "you declined 1.3.2" — so publishers SHOULD keep it meaningful, but nothing about resolution or caching depends on it moving.

Publishers MUST set `kind` to match the subname label the manifest is written under: `app` under `app.<product_id>.<tld>`, `widget` under `widget.<product_id>.<tld>`, `worker` under `worker.<product_id>.<tld>`. Hosts MUST reject a manifest whose `kind` does not match the subname it was read from.

**Why one Worker, not per modality.** A Worker is the product's single background process, carrying its full Host-API surface (signing, notifications, chain access, long-lived caches). Those capabilities do not split cleanly along the boundaries between Pocket, Chat, and Input, and one bundle per surface would duplicate that surface area and make the product's on-chain signing identity ambiguous. `includes` only advertises which user-facing affordances the same process serves; the executable remains a single artifact.

### Executable structure (v1)

The subname's `contenthash` record points at the bytes; this section defines what those bytes contain. Runtime APIs a Host exposes to a running executable (chain access, message passing, lifecycle hooks, etc.) are out of scope here — those belong in per-modality runtime contracts.

**App and Widget.** Single-page web applications, packaged as a directory whose root contains an `index.html` file. The Host treats `index.html` as the entry point and loads it to launch the modality. Relative paths inside `index.html` (scripts, styles, images) resolve against the same Bulletin IPFS gateway root from which the executable was fetched.

**Worker.** A directory of JavaScript files. The executable manifest's `entrypoint` field names the entry-point module as a path relative to the directory root (e.g. `index.js`, `src/worker.js`). The Host loads that module into a JS worker runtime to launch the modality. Other files referenced from the entry module (static imports, dynamic-import paths, asset URLs) resolve against the same Bulletin IPFS gateway root.

### Subname convention

| Subname                     | Carries                    |
|-----------------------------|----------------------------|
| `app.<product_id>.<tld>`    | App executable manifest    |
| `widget.<product_id>.<tld>` | Widget executable manifest |
| `worker.<product_id>.<tld>` | Worker executable manifest |

A product MAY publish any combination of these subnames; absence of a subname means the product does not provide that executable.

For each executable type the Host can render, it MUST query the corresponding subname to discover whether the product provides that executable. A Host with no surface for an executable type (e.g. a CLI Host has no dashboard for widgets) MAY skip the corresponding subname.

### Corner cases

- **Any icon failure** — `cid` unreachable, `format` unrecognised, or bytes that do not decode as the declared `format`. Render a placeholder; do not sniff or auto-correct. The root manifest stays valid and the product remains launchable.
- **Unrecognised grant value in `trustedProducts`.** Ignore that value and keep the recognised ones; the root manifest stays valid. Same for a key naming a product that does not resolve — the entry is inert.
- **Executable `contenthash` unset, non-IPFS codec, or undecodable.** That executable cannot be launched; surface a diagnostic. The product stays discoverable and its other executables still launch.
- **Missing root manifest but present executable subnames.** Product is not discoverable; executables MUST NOT be launched.
- **Unknown `kind` in an executable manifest.** Skip that executable rather than fail the whole product.
- **`kind` does not match the subname label** (e.g. `kind: 'app'` read from `worker.<product_id>.<tld>`). Treat the executable as malformed and skip it; do not coerce to the subname's label.
- **Manifest payload exceeds the dotNS text-record budget.** dotNS rejects the write at wire level (see [Security](#security)), so Hosts never observe oversized records.

### Implementation basics

The parameters, constants, and transport mechanics in this section are shared by both Publisher and Host implementations. Each role uses a subset of them; the role sections that follow call out which.

#### Parameters

Everything in this list is a **parameter** the implementation accepts as input; chains and contract deployments may change between environments.

- **dotNS smart contract.** A smart contract called through Revive pallet (Asset Hub on current testnets). Publishers need the chain's RPC endpoint for both reads and writes; Hosts need it for dry-run reads only.
- **Contract addresses on the dotNS chain.** Publishers need the registry (`IDotnsRegistry`) and the content resolver (`IDotnsContentResolver`). Hosts need only the registry; the resolver address for any given node is discovered through `IDotnsRegistry.resolver(node)`, not configured.
- **Bulletin chain.** A separate Polkadot chain hosting the `TransactionStorage` pallet. Publishers submit upload extrinsics here; Hosts never contact Bulletin RPC directly.
- **Bulletin IPFS gateway.** HTTP base URL used to read bytes back from Bulletin by CID — e.g. `https://paseo-ipfs.polkadot.io` on testnets. Hosts use it to fetch executable and icon bytes; publishers use it for the Step 8 verify probe.
- **Signing key.** A Polkadot account key, used for dotNS and Bulletin transactions via the standard Polkadot transaction flow. Publishers only — Hosts only read and need no signing key.

#### Constants

Fixed by the Bulletin chain protocol; identical across every publisher and Host.

| Constant                  | Value                    | Meaning                                     |
|---------------------------|--------------------------|---------------------------------------------|
| CID version               | `1`                      | CIDv1                                       |
| Multihash code            | `0xb220` (`blake2b-256`) | Hash algorithm used to derive the CID       |
| Digest length             | `32` bytes               | Output size of the BLAKE2b-256 digest       |
| Multicodec — single blob  | `0x55` (`raw`)           | Bytes stored and addressed as a raw payload |
| Multicodec — archive root | `0x70` (`dag-pb`)        | Root of a merkleized UnixFS directory       |

A blob stored as one block is `CIDv1(raw, blake2b-256(data))`. A directory is merkleized into a UnixFS DAG and serialised as a CAR, and its **root CID carries `dag-pb`, not `raw`** — a raw block holds no links, so it cannot be a DAG root. That is why `store_with_cid_config` takes `codec` as a call parameter rather than assuming one.

Both codecs encode to a single varint byte, so a CID's encoded length is the same either way, which keeps the publisher's Step 2 size preflight deterministic.

#### dotNS transport

**Names and nodes.** Prose in this RFC speaks in **subnames** — dotted labels like `widget.game.dot`. The dotNS contract API does not: every read and write addresses a node by its **subnode**, the ENS-style namehash of the dotted label as a `bytes32`. Implementations compute `namehash(subname)` once at each call site and pass the resulting `bytes32` into the contract call. Calls that accept a parent node plus a child label (`setSubnodeOwner`, `setSubnodeResolver`) take the parent's `bytes32` namehash directly and the child label as a string; the contract derives the child subnode internally.

Every dotNS contract call is composed as ABI-encoded calldata and dispatched through the dotNS chain's `pallet-revive`:

- **Reads** (`owner`, `resolver`, `text`, `contenthash`): wrap the calldata in a `ReviveApi.call(origin, ...)` dry-run RPC; ABI-decode the result. The dry-run requires an `origin` account, but nothing is signed, charged, or mutated — so it MUST NOT be a real keypair such as `//Alice` (using one couples reads to an account that may be unfunded, unknown, or absent in a given environment). Instead, derive the deterministic **Revive system account** using Substrate's standard `PalletId` account convention: the 4-byte tag `modl`, followed by the 8-byte `pallet-revive` ID `py/reviv`, zero-padded to a 32-byte `AccountId`. This account need not exist or hold a balance; it only names the dry-run caller, keeping reads environment-independent.

The 32-byte derivation (Rust):

```rust
fn pallet_account(pallet_id: &[u8; 8]) -> [u8; 32] {
    let mut account = [0u8; 32];
    account[..4].copy_from_slice(b"modl");
    account[4..12].copy_from_slice(pallet_id);
    account
}

let account = pallet_account(b"py/reviv");
// 0x6d6f646c70792f7265766976000000...0000
```

- **Writes** (`setResolver`, `setSubnodeOwner`, `setSubnodeResolver`, `setText`, `setContenthash`): the same calldata is sent as a signed Substrate extrinsic that invokes `pallet-revive::call(...)`. Fees and nonce are handled by the normal transaction flow. Publishers only.

### Publisher implementation

The **publisher** is the entity that publishes a product — a CLI, build script, GitHub Action, web UI, IDE plugin, or any other form running autonomously or in tandem with a developer who supplies the signing key. Any form is valid provided it executes Steps 1-8 below against the parameters and transport defined above.

#### Step 1 — Read the local product config

The publisher reads a local config file authored and source-controlled by the developer. The on-disk encoding (e.g. JSON, YAML, TOML) is a tooling decision and not normative. As an illustration of the shape the publisher needs in hand before Step 2, a hypothetical TypeScript form:

```typescript
type LocalProductConfig = {
  productName: string; // dotNS base name (e.g. "game.dot").
  displayName: string; // Human-readable product name (e.g. "Game"). Becomes RootManifest.displayName verbatim.
  description: string; // Short description of the product.
  icon: string; // Path to the icon file on disk. Uploaded to Bulletin, which stores raw bytes with no metadata — so the publisher MUST also surface the icon's format (`"jpeg"` | `"png"`) for the manifest's `Icon.format` field, either as a separate config field or by deriving it from the file extension. This RFC does not specify which approach is taken.
  trustedProducts?: Record<string, Granted[]>; // Same shape as RootManifest.trustedProducts; keys are bare product ids.
  app?: AppConfig;
  widget?: WidgetConfig;
  worker?: WorkerConfig;
};

type AppConfig = {
  root: string; // Path to the executable directory on disk.
  appVersion: SemVer; // Same SemVer tuple as the matching ExecutableManifest.
};

type WidgetConfig = {
  root: string; // Path to the executable directory on disk.
  appVersion: SemVer; // Same SemVer tuple as the matching ExecutableManifest.
  description?: string; // Optional tagline shown on the widget card.
  dimensions: {
    height: number[]; // Supported grid-step heights the widget can render at.
    width?: number; // Grid-step width. Optional; defaults to 1 column.
  };
};

type WorkerConfig = {
  root: string; // Path to the executable directory on disk.
  appVersion: SemVer; // Same SemVer tuple as the matching ExecutableManifest.
  entrypoint: string; // Path to the worker entry module inside the executable directory.
  includes: { // Same shape as WorkerManifest.includes; all may be false for a background-only worker.
    chat?: boolean;
    pocket?: boolean;
    input?: boolean;
  };
};
```

Each executable field (`app`, `widget`, `worker`) is optional — omitting it means that executable is not part of this publish operation.

#### Step 2 — Validate the local config

The publisher validates the local config before any network I/O:

- All referenced files (icon, executables) exist and are readable.
- Icon `format` is one of the values allowed by `Icon.format`. This stays strict on the publishing side: Hosts tolerate an unrecognised `format`, publishers MUST NOT emit one.
- Every `trustedProducts` key is a bare `<product_id>` label carrying no TLD suffix, and every grant value is one defined by `Granted`. Strict on the publishing side, as with the icon format: Hosts ignore unrecognised grants, publishers MUST NOT emit them.
- `appVersion` is a 3- or 4-element tuple of the right shape.
- Each executable's kind-specific fields are present, well-typed, and satisfy schema-level constraints.
- Pessimistic size preflight: compose the root manifest with a placeholder icon CID of the fixed encoded length (per the Constants table), and compose each executable manifest exactly — they carry no CID, so their size is already final. Abort if any composed manifest exceeds the dotNS text-record budget.

Local validation failures abort the publish with a human-readable error. No partial state is written on-chain.

#### Step 3 — Preflight on-chain state

Before submitting any Bulletin transactions, the publisher confirms both chains are ready: the dotNS records that will be written can be written, and the signing key is authorized to submit Bulletin uploads.

##### 3.1 Ownership of the base name

Read `IDotnsRegistry.owner(namehash("<product_id>.<tld>"))`. If it is not the publisher's signing-key address, abort — this covers both `address(0)` (unregistered; acquisition is out of scope) and any other owner (belongs to someone else).

##### 3.2 Resolver on the base name

The dotNS registrar installs the **reverse-resolver** contract as every fresh node's default resolver. The reverse resolver only implements `nameOf` / `setReverseName` — it cannot store text records — so the publisher MUST redirect the slot to the content resolver.

Read `IDotnsRegistry.resolver(namehash("<product_id>.<tld>"))`; if it isn't the content-resolver address from [Implementation basics](#implementation-basics), call `IDotnsRegistry.setResolver(namehash("<product_id>.<tld>"), <content-resolver address>)`. On re-publish this is a no-op read.

##### 3.3 Subnames for each executable

For each executable being published, ensure the corresponding subname (`app.<product_id>.<tld>`, `widget.<product_id>.<tld>`, or `worker.<product_id>.<tld>`) exists with the publisher as owner. If not, call `IDotnsRegistry.setSubnodeOwner({ parentNode, subLabel, parentLabel, owner })` with the publisher's address as owner.

`setSubnodeOwner` installs the reverse resolver on fresh subnodes (same default as base names), so each subnode also needs its resolver redirected: call `IDotnsRegistry.setSubnodeResolver({ parentNode, subLabel, parentLabel, resolver: <content-resolver address> })` (or `setResolver(subnode, …)` once the subnode is owned by the publisher). The subnode's resolver holds both the `executable` text record and the `contenthash` written in Step 7.

**Idempotency.** If a subnode already exists with the publisher as owner, skip the creation call. If it is owned by another account, abort — the publisher MUST NOT modify subnames it does not own.

##### 3.4 Bulletin storage authorization

Confirm the signing key is authorized to submit `TransactionStorage.store_with_cid_config(...)` extrinsics on the Bulletin chain. Read storage map `TransactionStorage.Authorizations` keyed by the enum variant `Account(<signer-address>)`:

```typescript
const auth = await api.query.TransactionStorage.Authorizations.getValue(Enum("Account", signerAddress));
```

The returned record has shape `{ extent: { transactions_allowance, transactions, bytes_allowance, bytes }, expiration }`, or `undefined` if the account has never been authorized. The signing key is usable iff:

- the record exists,
- `expiration > currentBlock`, and
- the remaining capacity (`transactions_allowance − transactions`, `bytes_allowance − bytes`) covers the planned upload (chunks-needed and total-bytes for the icon + every executable in this publish operation).

Granting is out-of-band via `TransactionStorage.authorize_account({ who, transactions, bytes })`. On testnet, the well-known `//Alice` keypair holds the authorization authority and a testnet publisher MAY self-grant. On mainnet `//Alice` has no such authority; authorization MUST come from a production-role account, and the request mechanism (portal, governance proposal, operator extrinsic, …) is **TBD** alongside the mainnet Bulletin deployment.

#### Step 4 — Upload assets to the Bulletin chain

The Bulletin chain's transaction-storage pallet stores one chunk per signed extrinsic, returning a content identifier derived from the bytes under the given `codec` / `hashing`:

```
TransactionStorage.store_with_cid_config({ cid: { codec, hashing }, data })
```

Larger artifacts are merkleized first: bytes are chunked and arranged into a Merkle DAG with a single root CID (serialised as a CAR — Content-Addressed aRchive). Two kinds of artifacts go through the same flow:

1. **The product icon.** Read the icon file from disk, merkleize (typically a single chunk for small images), and upload each chunk via `store_with_cid_config`. The resulting root CID becomes the root manifest's `icon.cid`.
2. **Each executable.** For each executable type (App, Widget, Worker) being published, read the executable directory, merkleize into a CAR, and upload each chunk via `store_with_cid_config`. The chain does not deduplicate server-side, so the publisher MUST probe each chunk's CID against the chain or its IPFS gateway and skip any that are already present. The resulting root CID is written to that executable's subname `contenthash` in Step 7.

Assets that fail to upload abort the publish. Re-running the publish is safe: chunks already on-chain are re-addressable by their CID and skipped on retry.

#### Step 5 — Compose manifests

With every CID in hand, the publisher constructs:

- One **root manifest** JSON conforming to `RootManifest`, with the icon's `cid` and `format` substituted in.
- One **executable manifest** JSON per executable conforming to the matching `AppManifest` / `WidgetManifest` / `WorkerManifest` shape. Executable manifests reference no CID; the artifact is bound to the subname's `contenthash` in Step 7.

All payloads start with `$v: 1`.

#### Step 6 — Validate the manifests

Before any dotNS write, the publisher:

1. Parses each composed JSON back through the v1 JSON Schema to confirm conformance.
2. Computes the UTF-8 byte length of each manifest and rejects any that exceed the dotNS text-record budget (the exact figure is still TBD — see [Unresolved Questions](#unresolved-questions)).

Either check failing aborts the publish before on-chain writes begin (see [Security § Size cap at publishing](#security)).

#### Step 7 — Write the manifests

`IDotnsContentResolver.setText(node, key, value)` and `setContenthash(node, value)` are **hard overrides**: each overwrites the previous value in full, no merge or history. Re-publishing replaces the previous manifests; the Bulletin bytes they referenced remain on-chain, unreferenced.

To enable rollback, the publisher first **snapshots** every record it will touch by reading the current value via `IDotnsContentResolver.text(node, key)` / `contenthash(node)`:

| Node                                    | Record                | Snapshot of                         |
|-----------------------------------------|-----------------------|-------------------------------------|
| `namehash("<product_id>.<tld>")`        | text `manifest`       | Existing root manifest (if any)     |
| `namehash("app.<product_id>.<tld>")`    | text `executable`     | Existing App executable manifest    |
| `namehash("app.<product_id>.<tld>")`    | `contenthash`         | Existing App executable CID         |
| `namehash("widget.<product_id>.<tld>")` | text `executable`     | Existing Widget executable manifest |
| `namehash("widget.<product_id>.<tld>")` | `contenthash`         | Existing Widget executable CID      |
| `namehash("worker.<product_id>.<tld>")` | text `executable`     | Existing Worker executable manifest |
| `namehash("worker.<product_id>.<tld>")` | `contenthash`         | Existing Worker executable CID      |

The table shows a publish covering all three executables; an executable absent from this publish has its rows left untouched. Empty values indicate first-time publish for that record.

The publisher then submits one write per row: `setText(...)` carrying the newly composed JSON, and `setContenthash(...)` binding each executable's uploaded root CID to its subname. The writes are independent and SHOULD be batched into a single signed extrinsic via `Utility.batchAll`, so every record lands in a single block or the entire batch fails atomically — a manifest visible without its `contenthash` describes an executable a Host cannot fetch.

**Rollback on partial failure.** If any write fails after a previous one succeeded, the publisher MUST restore every record already overwritten this run from its snapshot, then abort with a diagnostic naming the final state of every record. The product ends up either fully restored to the pre-publish state, or in a documented intermediate state the developer can recover from manually.

#### Step 8 — Verify

After all writes confirm, the publisher re-runs the resolution flow described in [Host implementation](#host-implementation) against the base name and asserts:

- Every manifest is readable via `text(node, key)` and matches the JSON the publisher just wrote.
- Every manifest round-trips through schema validation.
- Every executable subname's `contenthash` decodes to the root CID just uploaded for that executable.
- The root manifest's `icon.cid` and every executable CID are reachable on the Bulletin chain. Two equivalent probes: an HTTP `HEAD <gateway>/ipfs/<cid>` returning 2xx, or an on-chain read of `TransactionStorage.TransactionByContentHash(Blake2_128_Concat(content_hash))` returning a non-empty entry. The HTTP probe is faster; the on-chain probe is authoritative.

If any assertion fails, treat as a write failure: trigger the snapshot-restore path from Step 7's *Rollback on partial failure*, then abort with a diagnostic.

### Host implementation

How a Host resolves a product, from a dotNS name to validated manifests and launchable executable bytes. The parameters and transport used here are defined in [Implementation basics](#implementation-basics); a Host needs the dotNS smart-contract RPC endpoint, the registry address, and the Bulletin IPFS gateway, but no signing key (every dotNS call below is a dry-run, and Bulletin fetches go through HTTP).

#### Resolving a product

For a base name `B`:

1. **Compute the node hash.** `node = namehash(B)` using the ENS-style namehash algorithm. `bytes32`.
2. **Find the resolver.** Read `IDotnsRegistry.resolver(node)`. Returns the address of the resolver holding records for the node. `address(0)` → product does not exist; surface a diagnostic.
3. **Read the root manifest.** Read `IDotnsContentResolver.text(node, "manifest")` on the resolver from step 2. Returns the inline manifest JSON string. An empty string indicates the product does not exist; this also covers the case where the resolver is still the dotNS-issued default (the reverse resolver does not implement `text`).
4. **Parse and validate the root manifest.**
  - Parse the string as JSON. Failure → malformed; surface a diagnostic.
  - Validate `$v`. Unknown version → undiscoverable; surface a diagnostic and keep working for other products.
  - Validate against the v1 `RootManifest` schema. Validation failure → malformed; do not partially trust the result.
  - Two things are exempt and MUST NOT fail validation: an unrecognised `icon.format`, and an unrecognised grant value in `trustedProducts` (see [Root manifest (v1)](#root-manifest-v1)).
5. **(Optional) Read the author.** Hosts that surface authorship in UI call `IDotnsRegistry.owner(node)` — a single call that returns the canonical owner. The registry transparently handles the ERC-721 fallback for second-level names, so callers do not need to distinguish that case from subnodes.
6. **Probe executable subnames.** For each of `app.<product_id>.<tld>`, `widget.<product_id>.<tld>`, `worker.<product_id>.<tld>` whose executable type the Host can render (per [Subname convention](#subname-convention)): compute the subnode's namehash and repeat steps 2-4 against it, using `text(subnode, "executable")` instead of `text(node, "manifest")` and parsing against the matching `ExecutableManifest` variant. A subnode that does not exist (resolver `address(0)`) or has an empty text record means the product does not provide that executable — this is not an error.
7. **(Optional) Verify subname provenance.** By convention executable subnames are owned by the same account as the base name; the registry does not enforce that. A Host that needs strict provenance MUST call `owner(subnode)` for each present subname and verify equality with the base-name owner from step 5.
8. **Fetch executable bytes before launching.** For each executable manifest the Host intends to launch:
  - Read `contenthash(subnode)` on that subnode's resolver from step 6 and decode it to a CID. An unset slot, a non-IPFS codec, or bytes that fail to decode → cannot launch that executable; surface a diagnostic.
  - `GET <gateway>/ipfs/<cid>` from the Bulletin IPFS gateway. A CID that cannot be fetched → cannot launch that executable; surface a diagnostic.
  - v1 specifies no byte-level verification step (see [Unresolved Questions](#unresolved-questions)).

For the optional icon-byte fetch, the same gateway mechanics apply against the root manifest's `icon.cid` — no `contenthash` read, since the icon CID is carried in the manifest itself. A missing or undecodable icon is a UX degradation, not a launch blocker (see [Corner cases](#corner-cases)).

**Cache invalidation.** dotNS provides no push notifications, so a Host polls — for the content, not the metadata. A Host keeps each executable's manifest fields together with the `contenthash` it resolved at; that pair is the installed executable. Re-reading `contenthash(subnode)` and comparing detects a new deployment: a different value means new bytes, an equal value means the installed copy is current, and a subname that no longer resolves is not drift. Bytes are then cacheable indefinitely by the CID that addressed them, so a fetch is needed only when the `contenthash` moves.

A re-publish therefore never has to touch the manifest JSON — the `contenthash` moves whenever the bytes do. What a Host does with an observed change, fetch it or hold the installed copy until the user accepts, is Host policy and out of scope here.

Manifests carry no such signal, so re-read them on whatever schedule suits. One consequence: a revoked trust grant stays in force until the root manifest is re-read, so Hosts SHOULD bound how long a cached `trustedProducts` is honoured.

#### Conformance fixtures

A conforming Host implementation should produce well-defined behaviour for each of:

- Base name with no resolver (`resolver(node) == address(0)`) → product does not exist.
- Base name whose resolver is still the dotNS-default reverse resolver → product does not exist.
- Empty `text(node, "manifest")` → product does not exist.
- Malformed JSON in `manifest` → diagnostic; do not launch.
- Unknown `$v` in `manifest` → diagnostic; treat as undiscoverable.
- Root manifest fails `RootManifest` schema → diagnostic; do not launch.
- Unknown `icon.format` → root manifest still validates; render placeholder; product remains launchable.
- Unknown grant value in `trustedProducts` → root manifest still validates; that value is ignored, recognised values in the same entry still apply.
- `trustedProducts` key naming a product that does not resolve → entry is inert; root manifest still validates.
- `trustedProducts` key written with a TLD suffix (`"wallet.dot"` instead of `"wallet"`) → resolves to `wallet.dot.<tld>`, does not exist, entry is inert; grants nothing.
- Icon CID unreachable → render placeholder; product remains launchable.
- Icon bytes do not decode as the declared `format` → render placeholder; product remains launchable.
- Executable subname absent or empty `executable` text record → product does not provide that executable.
- Executable manifest fails its `ExecutableManifest` variant schema → skip that executable.
- Unknown `kind`, or `kind` does not match the subname label → skip that executable.
- Executable subname `contenthash` unset, non-IPFS codec, or undecodable → cannot launch that executable; surface a diagnostic.
- Executable CID unreachable → cannot launch that executable; surface a diagnostic.
- Executable subname owned by a different account than the base name (when strict provenance is enabled) → skip that executable.

## Drawbacks

- **JSON over a binary codec.** Costs text-record budget that a binary format would not — accepted for parseability with off-the-shelf tooling.
- **No oversized-manifest fallback.** A publisher who exceeds the dotNS text-record budget MUST shrink the payload.
- **Multiple lookups per resolution.** A full resolution for a product with all three executable types costs ~11 dotNS reads plus up to 4 Bulletin fetches. Mitigate with parallelisation and caching.
- **Schema evolution locks out older Hosts.** A new `$v` is invisible to Hosts that do not yet recognise it. A co-versioning scheme is left to a follow-up RFC.

## Alternatives

- **Binary codec (SCALE/protobuf).** Lower wire cost but requires a codec library in every consumer. JSON with off-the-shelf parsers is simpler and fits within dotNS text-record budgets.
- **Single manifest per product.** Fewer lookups, but a single record grows with each executable type and cannot be independently updated.

## Security

- **Trust anchor.** The dotNS name is the identity; the root manifest's `icon.cid` and each subname's `contenthash` bind that identity to specific bytes on Bulletin. That binding is only as strong as the fetch path: v1 defines no Host-side check that fetched bytes hash back to the `cid` they were requested by, so a Host trusts its Bulletin IPFS gateway to serve the addressed bytes. Byte-level verification is deferred (see [Unresolved Questions](#unresolved-questions)); until it lands, gateway selection is a security decision.
- **Icon supply chain.** Icon bytes are decoded and drawn, never executed, so the containment control is the render surface: Hosts MUST render icon bytes through a sandboxed image surface (e.g. `<img src="data:…">`) and never through a path that interprets them as markup or script. That MUST holds whatever the declared `format` says, which is why an unrecognised `format` is a rendering decision rather than a validity one — a Host that cannot decode a format renders a placeholder.
- **Trust grants are publisher-declared, not user-declared.** A grant is authenticated by nothing stronger than dotNS ownership, so a compromised or transferred name widens access with one `setText`. Two constraints follow. A grant MUST NOT override a denial the user already gave — it waives the publisher's prompt, never the user's. And revocation is a record edit with no signal attached, so a Host that honours a cached grant indefinitely cannot be revoked from (see [Cache invalidation](#resolving-a-product)).
- **Size cap at publishing.** The publisher MUST validate every manifest against the v1 schema and reject payloads exceeding the dotNS text-record budget before submitting. dotNS enforces a wire-level cap on writes.
- **Subname squatting is structurally prevented.** `setSubnodeOwner` is gated by parent-ownership: only the owner of `<product_id>.<tld>` can create the modality subnames.
- **No user data.** The manifest carries no user data; privacy exposure is limited to whatever dotNS RPC traffic reveals about which products a client is resolving.

## Unresolved Questions

- **Text-record byte budget on dotNS.** The hard ceiling on a manifest's size. Owner: dotNS team Proof-of-Concept.
- **Byte-level CID verification.** v1 requires no Host-side check that fetched bytes hash back to the `cid` they were fetched by, for either executables or icons — no Host implements one today, and specifying an unimplemented MUST buys nothing. Integrity therefore rests on the Bulletin IPFS gateway. Whether Hosts verify, at what obligation level, and what a mismatch does to a launch are deferred to a follow-up revision.

## Future Directions

- `Granted` covers the capabilities the Host runtime contracts name today. Further values — write access to storage, a scope of its own for signing — fit the same way: `all` stays the wildcard, and the array shape and the ignore-unrecognised-values rule let them land without a new `$v`.
- A manifest-aggregation RPC could eliminate the N+1 lookup pattern (one round-trip per subname) without changing the schema.
- A companion spec will pin down the dashboard grid (cell size, bounds, responsive behaviour) referenced by `WidgetManifest.dimensions`.
