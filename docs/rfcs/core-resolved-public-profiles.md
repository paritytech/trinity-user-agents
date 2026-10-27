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
name is the authenticity. Each entry is a kind and a payload; a reader decodes the kinds it
knows and keeps the rest, so adding a kind never changes the codec.

Kinds a reader may meet, and what their payload holds:

| Kind     | Payload                                                      |
|----------|--------------------------------------------------------------|
| `avatar` | `(cid: String, format: u8)`: a Bulletin CID, raw codec, blake2b-256; format indexes `png`, `jpeg`, `webp` |
| `name`   | the display name, raw UTF-8, at most 64 bytes                |
| `nft`    | `(instance: u64, collection: u32, item: u32, purse: [u8; 32])`, one per pinned item |
| `feed`   | `(kind: String, topic: [u8; 32], signer: [u8; 32])`, one per feed |

A tuple payload is SCALE and its strings carry their own length prefix; `name` is bare
bytes. A decoded record over 16 KiB is refused before parsing, and an `avatar` whose format
the reader does not know is treated as absent. Only `avatar` and `name` are resolved here;
`nft` and `feed` are carried so the layout is final.

Seity writes the record today, signing `setText` with the account that owns the name. Who
may write it, under which permission, and who pays, is a separate RFC.

## Approach

The design has four parts: the host call, the lookup, the avatar bytes, and when hosts ask.

**The host call.** A host asks the core for one person's profile by the label its contact
row already holds, and gets one of three answers.

```rust
/// A person's public profile as published under their dotNS name.
pub struct ProfileRecord {
	/// In the owner's order; a kind may repeat.
	pub entries: Vec<ProfileEntry>,
}

/// One element of a [`ProfileRecord`].
pub struct ProfileEntry {
	/// `avatar`, `name`, `nft`, `feed`, or a kind this reader skips.
	pub kind: String,
	/// The kind's payload.
	pub payload: Vec<u8>,
}

/// Payload of an `avatar` entry.
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: String,
	/// The image format, declared so no reader sniffs bytes.
	pub format: ProfileAvatarFormat,
}

/// The formats an avatar may be in.
pub enum ProfileAvatarFormat {
	Png,
	Jpeg,
	Webp,
}

/// What the core answers a host that asked for a person's profile.
pub enum PublicProfileLookup {
	/// The name has a record. Either field may be absent.
	Found { avatar: Option<ResolvedAvatar>, name: Option<String> },
	/// The name has no record, or none the core can read.
	None,
	/// The core could not tell: the chain or the bytes were out of reach.
	Failed { reason: String },
}

/// An avatar the core fetched and checked.
pub struct ResolvedAvatar {
	/// For change detection; the host stores it beside the bytes.
	pub cid: String,
	pub format: ProfileAvatarFormat,
	/// Hash to `cid`.
	pub bytes: Vec<u8>,
}

impl NativeTrUApiHostRuntime {
	/// `label` is the contact's username as the host stores it, `alice` or
	/// `alice.42`; the core appends the network's TLD. `force` skips the cache.
	pub async fn resolve_public_profile(&self, label: String, force: bool) -> PublicProfileLookup;
}
```

`None` and `Failed` are different answers on purpose. A host clears its stored avatar on
`None`, because the person took it down, and keeps it on `Failed`, because nothing is known.
A host never caches a `Failed`.

**The lookup.** The core walks dotNS as it does for a manifest: the protocol registry, the
network's TLD, `namehash(<label>.<tld>)`, the registry's resolver for that node, then
`text(node, "profile")`. A name still on the reverse resolver reverts, which is `None`. The
record is decoded in the core from the same definition the writer encodes with.

**The avatar bytes.** The core fetches them through the host's preimage backend
(`PreimageHost::lookup_preimage`, keyed by the CID's digest) and keeps them only if they hash
to the CID. A mismatch or a miss is an avatar the core does not have, not a `Failed` record:
the name and the CID still come back, and the host asks again next time. The core never maps
a CID to a gateway URL.

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
- The display name is returned and not drawn. Where it goes is a host decision.

## Open questions

- Whether the display name shows beside the username or instead of it.
