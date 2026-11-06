---
title: "Core-resolved public profiles"
owner: "@corey-hathaway, @pgherveou"
status: draft
---

# RFC — Core-resolved public profiles

## Summary

This RFC lets hosts show a contact's avatar instead of a letter tile. A person publishes their profile in a `profile`
text record under their dotNS username. The core reads that record and resolves the profile information it
points at, such as the avatar, for the host. Native hosts cache the record. The shared Rust resolver also builds for
WASM, where it reads the chain on each load; JavaScript bindings are deferred.

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
- **Host image cache**: Kingfisher on iOS, Coil on Android. Stores username-keyed avatars in memory and on disk
  for one hour, without extending their expiry on access.
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

Pairing and signing hosts load avatars through the [image loader](image-loader.md):

```rust
runtime.load_image(ImageReference::Username { username, force }).await
```

The result is `Result<LoadedImage, ImageError>`. The core resolves the profile,
selects the first understood avatar, and loads its CID through the same verified
Bulletin path as an explicit CID reference. The result carries the bytes, their
verified format, and their CID as a content key for the host's disk byte cache.
`force` bypasses the native profile record cache; image verification always applies.
The shared Rust loader builds for WASM and reads the profile from the chain on
every load, regardless of `force`.

A profile without an understood avatar returns `ImageError::Unavailable`
without a Bulletin request. Invalid usernames are `Unsupported`.
Image policy failures are `Rejected`; chain or database failures
are `Unavailable`. A later WASM call reads the current profile. Native calls
refresh after the record expires or with `force`; the loader does not poll for a
profile update.

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

The host draws a letter tile while its shared image-library adapter calls
`load_image` with a username reference. A failed load leaves the tile visible
and is not stored as an image. Successful loads are cached under
`public-profile:<username>` with a persisted one-hour expiry. Memory and disk
entries share that deadline; reading an entry or restarting the app does not
extend it.

Opening a chat refreshes the username reference with `force`, bypassing the
host's image cache and the core's record cache, and replaces the cached image
on success. dotNS emits no event on a text change, so outside
an opened chat a new avatar shows once the host refreshes its reference and
the core's record has expired.

```text
Native UI / image library         Core profile cache + image loader       dotNS / Bulletin
  |                                      |                                      |
  |-- load_image(Username) -------------->|                                      |
  |                                      |-- read profile if stale or forced -->|
  |                                      |<-- profile record -------------------|
  |                                      |-- cache record for one hour          |
  |                                      |-- bitswap_v1_get(CID) --------------->|
  |                                      |<-- bytes ----------------------------|
  |                                      |-- verify CID and image policy        |
  |<-- LoadedImage { bytes, format, CID } -|                                      |
  |-- decode; cache by username (1h)      |                                      |
  |                                      |                                      |
  |-- load_image(Username without avatar)>|                                      |
  |                                      |-- resolve/cache record as above      |
  |<-- ImageError::Unavailable -----------|                                      |
  |-- keep letter tile                   |                                      |
```

## Trade-offs

- Profiles are read on refresh, not pushed. Fast-changing data does not belong in the record.
- After Bulletin drops a blob, a cached avatar remains available until its expiry. Further loads require the publisher
  to upload it again.
- When the chain is out of reach, an uncached load shows the letter tile. Native hosts can use a cached profile until it expires.
