---
title: "Core-resolved public profiles"
owner: "@corey-hathaway, @pgherveou"
status: draft
---

# RFC — Core-resolved public profiles

## Summary

This RFC lets hosts show a contact's avatar instead of a letter tile. A person publishes their profile in a `profile`
text record under their dotNS username. The core reads that record and resolves the profile information it
points at, such as the avatar, for the host. The native core caches the record for one hour. Hosts separately cache
verified image bytes by CID, so refreshing a profile does not download an unchanged avatar. The shared Rust resolver
also builds for WASM, where it reads the chain on each lookup; JavaScript bindings are deferred.

## Motivation

Contacts are letter tiles because hosts do not read the profile a person publishes under their dotNS name. Resolving it
in the core gives hosts one record decoder and cache. The [image loader](image-loader.md) reads avatars over the core's
Bulletin connection and applies one image policy across features.

## Definitions

- **Username**: a person's dotNS name without the TLD, such as `alice.01`.
- **Profile record**: the `profile` text record under a username, holding a `ProfileRecord`.
- **Publisher**: the app that writes the profile record and uploads the avatar to Bulletin, Seity today.
- **Avatar**: the image a profile record points at, by Bulletin CID and format.
- **Bulletin**: where the avatar bytes live, for 14 days after upload.
- **Core cache**: the native core's SQLite table of profile records, one row per username, kept for one hour.
- **Host image cache**: Kingfisher on iOS, Coil on Android. Stores verified images by CID in memory and on disk.
  The bytes are immutable, so capacity limits and eviction determine retention, not a freshness timer.
- **Letter tile**: the placeholder a host draws for a contact, the first letter of the username, with the avatar drawn
  over it once loaded.

## Approach

### The record

The `profile` text record holds a SCALE-encoded `ProfileRecord`, base64, in plaintext. Owning the name is the
authenticity.

```rust
use subxt::utils::WrapperKeepOpaque;
use truapi::{BulletinCid, ImageFormat};

/// A person's public profile as published under their dotNS name.
#[derive(Encode, Decode)]
pub struct ProfileRecord {
	/// In the owner's order.
	pub entries: Vec<WrapperKeepOpaque<ProfileEntry>>,
}

/// The kinds of entry a record may hold. More kinds will be added; a reader keeps the
/// ones it does not know as opaque bytes. Indices are never reused.
#[derive(Encode, Decode)]
pub enum ProfileEntry {
	#[codec(index = 0)]
	Avatar(ProfileAvatar),
}

/// Payload of an `Avatar` entry.
#[derive(Encode, Decode)]
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: BulletinCid,
	/// Declared by the writer.
	pub format: ImageFormat,
}
```

`WrapperKeepOpaque<T>` stores each entry as SCALE length-prefixed bytes. Readers decode an entry only when they understand its full payload, so new entry kinds and extended payloads survive unchanged when an older reader rewrites the record.

`BulletinCid` encodes as its 32-byte blake2b-256 digest.
`ImageFormat` is shared with the image loader, with SCALE indices PNG=0, JPEG=1,
WebP=2 and GIF=3. An incomplete avatar, an unknown format or an extended
payload leaves the entry opaque and unchanged.

Reader rules:

- A record over 16 KiB, or with trailing bytes, is refused.
- When a record holds several entries of the same kind, the first understood one is used.
- Avatar bytes must pass the shared [image policy](image-loader.md#approach), including the record's declared format.

A writer keeps every entry it does not understand. Who may write the record and who pays is out of scope. Keeping the
avatar on Bulletin past its 14 day retention is the publisher's job.

### The host calls

Pairing and signing hosts resolve profile metadata before asking their image library for an image:

```rust
pub struct PublicProfile {
    pub avatar: Option<ProfileAvatar>,
}

let profile = runtime.resolve_public_profile(&username, force).await?;
```

The result is `Result<PublicProfile, PublicProfileError>`. The core selects the first
understood avatar from the record. A missing or malformed record, or one without an
understood avatar, returns `PublicProfile { avatar: None }`. Invalid usernames and
chain or database failures return an error. Resolving a profile never reads Bulletin.
`force` bypasses only the native profile record cache. The WASM resolver reads the
chain on every lookup, regardless of `force`.

The host looks up the returned CID in its image cache. Only a cache miss calls the
shared [image loader](image-loader.md):

```rust
runtime.load_image(ImageReference::Bulletin {
    cid: avatar.cid,
    declared_format: Some(avatar.format),
}).await
```

This returns `Result<LoadedImage, ImageError>`. The core reads the bytes from Bulletin
and checks the CID, format and image policy. The host stores a successful result under
its CID. Unsupported references, rejected images and unavailable bytes remain image
loader errors. Profile and image errors leave the host's placeholder visible.

The core derives `namehash(<username>.<tld>)` and reads `text(node, "profile")`
on the `contentResolver` contract registered by the dotNS protocol registry.
The username's node resolver does not select its profile storage. Independent
registry views run concurrently on one pinned block through the shared EVM
contract caller.

### The core cache

The native core keeps each record in its SQLite database for one hour. The WASM core has no profile record cache.

```sql
CREATE TABLE public_profiles (
    username      TEXT PRIMARY KEY,
    public_record BLOB NOT NULL,
    expires_at    INTEGER NOT NULL
);
```

`public_record` is the SCALE-encoded `ProfileRecord`, so a name without a record is cached too, as an empty one.

- A row that has not expired is returned without touching the chain.
- An expired or missing row, or `force`, reads the chain and replaces the row. Errors are not cached.
- Expired rows are deleted before each write. The table holds at most 1000 rows; past that the rows
  closest to expiry are deleted first.

### The host

The host resolves the profile on each avatar load. A fresh native profile-cache entry
avoids a dotNS read. The returned CID selects the image-library request, so an image
cache hit cannot skip profile resolution. Hosts do not keep a second username-to-image
freshness cache.

Coil and Kingfisher cache verified bytes by CID on disk and decoded images in memory;
processed variants may add size or processing details to their keys. A disk hit can
repopulate memory after a restart. There is no image freshness expiry: the bytes named
by a CID do not change. Library capacity limits, eviction and explicit cache clearing
still apply. After eviction, a later request fetches and verifies the bytes again.
Cached bytes must still match the profile's declared format.

Opening a chat resolves its profile with `force`, bypassing the core's record cache.
The host still checks the CID image cache. An unchanged CID reuses existing bytes;
a changed CID selects its own cached image or fetches it if absent. A profile without
an avatar shows the letter tile even if bytes for its previous CID remain cached.
Failures do not create profile or image cache entries. dotNS emits no text-change
event, so a displayed avatar changes on a later lookup, not at the expiry instant.

```text
Native avatar adapter            Core + SQLite                    dotNS
  |                                  |                              |
  |-- resolve_public_profile ------->|                              |
  |                                  |-- if stale or forced ------->|
  |                                  |<-- profile record -----------|
  |                                  |-- cache record for one hour  |
  |<-- PublicProfile { avatar } -----|                              |
  |                                  |                              |

Native avatar adapter       Coil / Kingfisher          Core image loader / Bulletin
  |                                  |                              |
  |-- image(CID, format) ------------>|                              |
  |                                  |-- check memory, then disk    |
  |<-- cached image on hit ----------|                              |
  |                                  |                              |
  |                                  |-- load_image(Bulletin) ------>| on miss
  |                                  |                              |-- fetch bytes
  |                                  |                              |-- verify CID/policy
  |                                  |<-- LoadedImage --------------|
  |                                  |-- cache by CID               |
  |<-- image ------------------------|                              |
```

## Trade-offs

- Profiles are read on refresh, not pushed. Fast-changing data does not belong in the record.
- The host makes a metadata lookup before each image request, but the core serves fresh native records locally.
- Cached CID bytes can remain usable after Bulletin drops the blob. A cache miss then requires the publisher to
  upload the blob again.
- When dotNS is out of reach, a fresh cached profile can still select a cached image. An expired profile lookup
  fails and leaves the placeholder; retaining old metadata on failure is not part of this cache policy.
