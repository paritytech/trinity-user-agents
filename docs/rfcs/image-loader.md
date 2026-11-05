---
title: "Image loader in the core"
owner: "@eugypalu"
status: draft
---

# RFC — Image loader in the core

## Summary

The core turns an image reference into verified bytes a host can draw, through one call. A reference is a Bulletin CID, or an on-chain source that names one, such as a dotNS profile record or NFT item metadata. The core reads the blob over its own Bulletin connection and checks it against the CID and an image policy.

## Motivation

Every feature that shows an image builds its own loader. iOS and Android each resolve chat card and Pocket face images, [core-resolved public profiles](https://github.com/paritytech/trinity-user-agents/pull/1709) adds a core resolver for contact avatars, and NFT purses ([#1509](https://github.com/paritytech/trinity-user-agents/issues/1509)) would add another. Each decides separately how bytes are fetched, verified, limited and cached. One entry point and one implementation replace them.

## Approach

The host calls `load_image(reference)` on the runtime through UniFFI or the WASM bridge. The call is async and cancellable: cancelling it stops the fetch. It returns the bytes, their format as read from the bytes, and a content key, the blob's CID, which hosts key their disk caches on. Otherwise it fails as `Unsupported` when the reference names nothing the core loads, `Rejected` when the bytes fail the policy, or `Unavailable` when Bulletin does not return them. Only `Unavailable` can succeed on a later attempt. A failed image draws as a placeholder without affecting the card or tree. The renderer wire type remains unchanged.

References the core loads, and where it finds each one's CID:

| Reference | CID source | Drawn on |
| --- | --- | --- |
| Bulletin CID, with the format its source declares, if any | the reference itself | renderer tree (chat card, Pocket face, face preview), product icon from the manifest |
| Username | the `profile` text record under the dotNS name | contact avatar |
| NFT item | the item's Scarcity metadata | NFT purse, purse confirmation, NFT chat message |

The design has three parts:

- **Policy**: Images must be PNG, JPEG, GIF or WebP, identified by their bytes, no larger than a Bulletin blob (2 MiB), and no larger than 4096 pixels per side. SVG is rejected everywhere. When the source declares a format, as a manifest icon or a profile record does, bytes in another format are rejected.
- **Bulletin**: Only raw blake2b-256 CIDs load. The core decodes the CID into its preimage key and reads the blob with `bitswap_v1_get` over the Bulletin connection it already holds for preimage submission. Bytes that do not hash to the key are discarded.
- **On-chain sources**: For a username, the core reads the `profile` record as [core-resolved public profiles](https://github.com/paritytech/trinity-user-agents/pull/1709) defines it. For an NFT item, the core resolves Scarcity metadata in order of precedence: instance, item, then collection. Only the collection owner can write this metadata. The `image` key names a Bulletin blob, while a Stash item's `manifest` key names Bulletin JSON containing a picture and thumbnail. The caller can ask for chain state to be read again rather than served from cache.

## Compatibility

This introduces breaking changes for products and host app testers. Once hosts adopt the loader, SVG, Bulletin CIDs other than raw blake2b-256, and images over 2 MiB no longer render. Most recent blobs on Paseo Bulletin have SHA2-256 CIDs, so an image among them renders only once it is stored again under a blake2b-256 CID. On iOS one Kingfisher provider, and on Android one Coil fetcher, call `load_image` in place of the per-feature fetchers. The core loader and both host switches merge together.

This also amends [unified-renderer.md](unified-renderer.md), where `ImageSource` lists SVG and fetches any CID through the gateway, and [core-resolved public profiles](https://github.com/paritytech/trinity-user-agents/pull/1709), whose avatars load through `load_image`.

## Trade-offs

- Archive images in a renderer tree and inline `data:` chat icons stay outside the loader. Moving archive resolution into the core is a separate RFC.
- Images stored as a DAG rather than a single Bulletin blob do not load until the core can read DAGs.
- The side limit bounds the memory one frame takes, not the number of frames in an animated GIF or WebP.
- Dropped: HTTP fetching in the core, which has no HTTP client; reading Bulletin through the host's `lookup_preimage`, which iOS and Android answer from an IPFS gateway; host-side byte verification; a byte cache in the core, which would keep a second copy of every image the host already caches; and widening the renderer wire type in place, which would cause deployed hosts to reject entire trees containing unknown variants.
