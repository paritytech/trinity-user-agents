---
title: "Core-resolved public profiles"
owner: "@corey-hathaway"
status: draft
---

# RFC — Core-resolved public profiles

## Summary

A person's public profile is one dotNS text record, `profile`, under their own name. The
core resolves it, verifies the avatar bytes and hands each host what to draw. Hosts decode
nothing and fetch from no gateway. Products cannot call this: it is the host's own call into
the core, like funding.

## Motivation

A contact in the chat list is a letter tile. The host knows the person's name and nothing
else, because there is no record a person can publish that every host reads.

dotNS already gives every name a set of text records, and every host already reads one of
them: a product's `manifest`. Seity now publishes a person's profile as a second text
record under their name, with the key `profile`. Reading it takes the walk hosts already
make for a manifest, and nothing new on chain.

If each host read the record itself there would be three decoders, in Swift, Kotlin and
TypeScript, that had to agree byte for byte, and three fetch paths. The core already has
the walk, a preimage backend on every host and one place that checks bytes against a CID,
so it reads the record once for all of them.

## The record

The record is a SCALE `ProfileRecord`, base64 in the text record, in plaintext. Owning the
name is the authenticity.

The record is a list of entries. Each entry is a `ProfileEntry`, a SCALE enum with fixed
indices, wrapped in `Skippable<T>`: the entry is written as a length-prefixed byte string,
and a reader that does not know the index keeps the bytes instead of failing. A kind added
later neither breaks an older reader nor is lost when that reader writes the record back.
The bytes are those of `frame_support::traits::WrapperKeepOpaque<T>`; the core does not
depend on `frame-support`, so the wrapper is defined here.

```rust
/// A person's public profile as published under their dotNS name.
#[derive(Encode, Decode)]
pub struct ProfileRecord {
	/// In the owner's order; a kind may repeat.
	pub entries: Vec<Skippable<ProfileEntry>>,
}

/// A length-prefixed `T` that a reader steps over when it cannot decode it.
pub enum Skippable<T> {
	/// Decoded, with no bytes left over.
	Known(T),
	/// Kept as read, so a writer that edits the record preserves it.
	Unknown(Vec<u8>),
}

/// The kinds of entry a record may hold. Indices are allocated in this document and
/// never reused.
#[derive(Encode, Decode)]
pub enum ProfileEntry {
	#[codec(index = 0)]
	Avatar(ProfileAvatar),
	/// The display name. At most 64 bytes of UTF-8; a reader ignores a longer one.
	#[codec(index = 1)]
	Name(String),
	#[codec(index = 2)]
	Nft(PinnedNft),
	#[codec(index = 3)]
	Feed(ProfileFeed),
}

/// Payload of an `Avatar` entry.
#[derive(Encode, Decode)]
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: String,
	/// Declared by the writer. Hosts do not sniff bytes, as for a manifest icon.
	pub format: ProfileAvatarFormat,
}

/// The formats an avatar may be in.
#[derive(Encode, Decode)]
pub enum ProfileAvatarFormat {
	#[codec(index = 0)]
	Png,
	#[codec(index = 1)]
	Jpeg,
	#[codec(index = 2)]
	Webp,
}

/// Payload of an `Nft` entry: one pinned item and the purse that should hold it. The
/// chain, not the record, says whether it is still there.
#[derive(Encode, Decode)]
pub struct PinnedNft {
	/// Scarcity's permanent instance id.
	pub instance: u64,
	/// Fixed at mint, so a reader can draw catalogue artwork before the instance is readable.
	pub collection: u32,
	/// Fixed at mint, as `collection`.
	pub item: u32,
	/// The account that should hold the item.
	pub purse: [u8; 32],
}

/// Payload of a `Feed` entry: a statement-store feed the profile points at.
#[derive(Encode, Decode)]
pub struct ProfileFeed {
	/// The feed kind, such as `mood`.
	pub kind: String,
	/// The statement topic every statement of the feed carries.
	pub topic: [u8; 32],
	/// The account whose proof a statement must carry to count.
	pub signer: [u8; 32],
}
```

Reader rules:

- A record over 16 KiB is refused before parsing. A record with trailing bytes is refused.
- An entry whose index is unknown, or whose payload does not decode as exactly one value,
  is `Unknown` and kept. It never fails the record. An `Avatar` in a format the reader
  does not know is such an entry: no avatar for this reader, and still in the record for
  the next writer.
- The first `Avatar` is the avatar. The first `Name` of at most 64 bytes is the name.
- Only `Avatar` and `Name` are resolved by this RFC. `Nft` and `Feed` are defined so the
  layout is final and a reader that does not use them still carries them.

Entry indices:

| Index | Entry    | Allocated by |
|-------|----------|--------------|
| 0     | `Avatar` | this RFC     |
| 1     | `Name`   | this RFC     |
| 2     | `Nft`    | this RFC     |
| 3     | `Feed`   | this RFC     |

A new kind is a new variant with the next free index, added to this table by the RFC that
proposes it. The index is claimed when that RFC is opened, so a product can write the
entry while the proposal is under review and no byte changes when it is accepted. An index
is never reused.

Seity writes the record today, signing `setText` with the account that owns the name. A
writer that updates its own entries keeps every entry it does not understand. Who may
write, under which permission, and who pays, is a separate RFC.

## Approach

The design has four parts: the host call, the lookup, the avatar bytes, and when hosts ask.

**The host call.** A host asks the core for one person's profile by the label its contact
row already holds.

```rust
impl NativeTrUApiHostRuntime {
	/// `label` is the contact's username as the host stores it, `alice` or
	/// `alice.42`; the core appends the network's TLD. `force` skips the cache.
	pub async fn resolve_public_profile(
		&self,
		label: String,
		force: bool,
	) -> Result<Option<PublicProfile>, PublicProfileError>;
}

/// The resolved parts of a person's record. Either field may be absent.
pub struct PublicProfile {
	pub name: Option<String>,
	pub avatar: Option<PublicAvatar>,
}

/// The avatar the record points at, with its bytes when the core could get them.
pub struct PublicAvatar {
	/// Identifies the image; the host stores bytes under it.
	pub cid: String,
	pub format: ProfileAvatarFormat,
	/// Hash to `cid`. `None` when the preimage was not available on this ask.
	pub bytes: Option<Vec<u8>>,
}

pub enum PublicProfileError {
	/// Not a label the core will resolve.
	InvalidLabel,
	/// The chain was out of reach, or the walk failed.
	Dotns(DotnsError),
}
```

The answers are distinct on purpose, and each tells the host what to do with what it
already holds:

| Answer                                         | Meaning                              | Host              |
|------------------------------------------------|--------------------------------------|-------------------|
| `Err(_)`                                       | Nothing is known.                    | Keep, retry later |
| `Ok(None)`                                     | No record under the name.            | Clear             |
| `Ok(Some(p))`, `p.avatar` is `None`            | A record, no avatar.                 | Clear the avatar  |
| `Ok(Some(p))`, avatar with `bytes` `None`      | An avatar the core could not fetch.  | Keep if the CID matches, retry the bytes |
| `Ok(Some(p))`, avatar with `bytes` `Some`      | The avatar.                          | Store under the CID, draw |

A host never caches an `Err`.

**The lookup.** The core walks dotNS as it does for a manifest: the protocol registry, the
network's TLD, `namehash(<label>.<tld>)`, the registry's resolver for that node, then
`text(node, "profile")`. A name still on the reverse resolver reverts, which is `Ok(None)`.
The record is decoded in the core from the same definition the writer encodes with.

**The avatar bytes.** The core fetches them through the host's preimage backend
(`PreimageHost::lookup_preimage`, keyed by the CID's digest) and returns them only if they
hash to the CID. A mismatch or a miss is an avatar without bytes, not an error: the name
and the CID still come back, and the core tries again on the next ask. The core never maps
a CID to a gateway URL, and does no image work; the host resizes for display.

**Caching.** The decoded record (CID, format, name, when it was read) is cached in core
storage for one hour. The bytes are not; the host keeps them, keyed by CID, because Bulletin
drops blobs after 14 days and the host's copy is the durable one. `force` reads the chain
regardless of the cache.

**When hosts ask.** On a contact the host has not checked, on foreground for contacts whose
last check is older than the cache lifetime, and with `force` when that contact's chat is
opened. Not on a timer in the background. dotNS emits no event for a text change, so there is
nothing to subscribe to.

## Trade-offs

- Reading is a poll at refresh. Anything that changes often, such as mood, belongs in a
  statement-store feed the record points at, not in the record.
- Every host persists avatar bytes it could re-fetch. Bulletin retention decides that.
- The core grows a decoder and a cache entry, in exchange for one implementation instead of
  three.
- Entry kinds are enum indices, not strings. A new kind costs a line in this document's
  table instead of a name a writer picks alone, in exchange for a closed set the codegen
  can emit and one byte per entry.
- `Skippable<T>` is new to truapi, which has so far kept forward compatibility with
  versioned enums and refused unknown versions. A record written by several products over
  time needs the other rule. The generated TypeScript sees each entry as bytes and decodes
  it lazily; how the codegen gets there is the implementer's call.
- The avatar format is declared, not sniffed, as the product-manifest RFC requires for a
  manifest icon. A reader that does not know the format shows no avatar.
- The display name is returned and not drawn. Where it goes is a host decision.

## Open questions

- Whether the display name shows beside the username or instead of it.
