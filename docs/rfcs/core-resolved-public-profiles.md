---
title: "Core-resolved public profiles"
owner: "@corey-hathaway, @pgherveou"
status: draft
---

# RFC — Core-resolved public profiles

## Summary

This RFC lets hosts show a contact's avatar instead of a letter tile. A person publishes their profile in a `profile`
text record under their dotNS username. The core reads that record and resolves the profile information it
points at, such as the avatar, for the host. The native core serves stored metadata immediately and refreshes records when explicitly forced or older than one hour. Hosts separately cache
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
- **Core cache**: the native core's SQLite table of profile records, one row per username, fresh for one hour and retained by last use within a 1,000-record limit.
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

Signing hosts observe a finite stream of profile metadata before asking their image library for an image:

```rust
let options = PublicProfileResolutionOptions {
    force_fresh: false,
    skip_stale: false,
};
let profiles = runtime.observe_public_profile(&username, options);
```

The stream carries `Result<PublicProfile, PublicProfileError>` and emits at most two
profiles. On native hosts it first emits a stored profile if one exists, including
stale metadata unless `skip_stale` is true. Both option fields are required. A fresh record
completes the stream without a chain lookup unless `force_fresh` is true. A forced
observation or a stale or missing record triggers one lookup and stores the result.
After a stored emission, the resolved profile is emitted only if it differs; an unchanged
result still renews the record's freshness. When a stale stored value was skipped,
the resolved profile is emitted even if it is unchanged, so the successful lookup
still produces one value. A fresh stored value remains eligible for emission even
when `skip_stale` is true. The stream then completes. Forcing
does not skip the initial stored value or bypass the host's image cache.

The core selects the first understood avatar from the record. A missing or
malformed record, or one without an understood avatar, produces
`PublicProfile { avatar: None }`, distinct from a cache miss or lookup failure.
Invalid usernames and chain or database failures terminate the stream with an
error. A lookup error after a cached emission leaves that cached value available
to the host. With a skipped stale value, lookup failure emits only the error.
Resolving a profile never reads Bulletin. The WASM resolver has no record cache
and emits one profile from the chain regardless of these options.

Native bindings deliver profiles, completion and errors through an observer and
return a cancellable subscription. Cancelling the subscription drops the
observation future. Both native apps collect profile updates outside their image
libraries, passing each emitted avatar to a separate image request.

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
loader errors. Initial profile and image errors leave the host's placeholder visible.

The core derives `namehash(<username>.<tld>)` and reads `text(node, "profile")`
on the `contentResolver` contract registered by the dotNS protocol registry.
The username's node resolver does not select its profile storage. Independent
registry views run concurrently on one pinned block through the shared EVM
contract caller.

### The core cache

The native core retains records independently of their one-hour freshness deadline.
The WASM core has no profile record cache.

```sql
CREATE TABLE public_profiles (
    username      TEXT PRIMARY KEY,
    public_record BLOB NOT NULL,
    expires_at    INTEGER NOT NULL,
    access_order  INTEGER NOT NULL
);
CREATE INDEX public_profiles_access_order ON public_profiles(access_order);
```

`public_record` is the SCALE-encoded `ProfileRecord`, including unknown entries. A
name without a record is cached too, as an empty one.

- Reading stored metadata advances its access order in the same transaction, without renewing freshness.
- A successful chain lookup replaces the full record, renews its one-hour freshness deadline and advances access order.
- Access order increases independently of wall-clock time, so simultaneous accesses and clock adjustments do not reorder eviction.
- Expired metadata remains available for the initial emission. The table holds at most 1,000 rows; writes evict the least recently used rows over that limit.
- Lookup errors do not replace stored metadata. Undecodable cache records count as missing.

### The host

The host resolves the profile on each avatar load. A fresh native profile-cache entry
avoids a dotNS read unless that observation explicitly forces one. The returned CID selects the image-library request, so an image
cache hit cannot skip profile resolution. Hosts do not keep a second username-to-image
freshness cache.

Coil and Kingfisher cache verified bytes by CID on disk and decoded images in memory.
Processed variants may also persist on disk, with format and processing details in
their keys. A disk hit can repopulate memory after a restart. There is no image
freshness expiry: the bytes named
by a CID do not change. Library capacity limits, eviction and explicit cache clearing
still apply. After eviction, a later request fetches and verifies the bytes again.
Cached bytes must still match the profile's declared format.

A streaming avatar adapter applies each profile to the image library. An unchanged
avatar needs no replacement request. A changed avatar selects its own cached image
or fetches it if absent, keeping the displayed image visible while the replacement
loads. A successful profile without an avatar restores the letter tile. A metadata
refresh error preserves the displayed image. Leaving the view cancels metadata
observation and image loading.

dotNS emits no text-change event, so a displayed avatar changes on a later
observation, not at the expiry instant. App callers explicitly set `skip_stale`
to false. A new iOS chat screen requests a forced
metadata observation. Repeated header rendering reuses that observation. Once it
completes or fails, later image-size changes use ordinary freshness. Cancellation
retains the pending forced intent for a restarted observation, while the
`skip_stale` choice is preserved. Contact, search and
chat-list cells explicitly choose ordinary freshness. Reappearance of a retained chat screen does not start another forced
observation. Android's composition-owned avatar collector forces metadata freshness
on conversation entry and uses ordinary freshness in lists and search. It updates
Coil request data for each emitted avatar without awaiting image loading. A
username change or removal of the composition cancels collection and image work.

```text
Native avatar adapter            Core + SQLite                    dotNS
  |                                  |                              |
  |-- observe_public_profile ------->|                              |
  |                                  |-- read and touch stored row  |
  |<-- stored PublicProfile ---------| if available                 |
  |                                  |                              |
  |                                  |-- forced, stale or missing ->|
  |                                  |<-- profile record -----------|
  |                                  |-- store and renew freshness  |
  |<-- resolved PublicProfile -------| only if different            |
  |<-- complete --------------------|                              |
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
- A streaming observation can show a stale cached profile while dotNS is out of reach. LRU eviction can remove that metadata independently of whether its image bytes remain cached.
