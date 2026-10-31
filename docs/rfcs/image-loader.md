---
title: "Image loader in the core"
owner: "@eugypalu"
status: draft
---

# RFC — Image loader in the core

## Summary

The core resolves product-selected image references into bytes that a host can render. The host identifies the surface and fetches raw bytes when requested, while the core validates them. This applies to Bulletin blobs, files in a product's Worker archive, inline `data:` images, and NFT item images.

## Motivation

Today, iOS and Android independently resolve images for chat cards and Pocket faces through gateway conversions, without verifying that the returned bytes match the CID. This allows a gateway to substitute arbitrary images for those selected by a product. There are also no limits on image size, format, or dimensions. NFT purses ([#1509](https://github.com/paritytech/trinity-user-agents/issues/1509)) would introduce yet another resolver.

## Approach

The host calls `load_image(product_id, surface, reference)` on the runtime through UniFFI or the WASM bridge. The core checks the reference against the surface's policy, requests the bytes through a host callback, validates them, and returns either `Ready` with the bytes and a content key or `Unavailable` with a reason. Failed images display a placeholder without affecting the card or tree. The renderer wire type remains unchanged.

Accepted references and byte cap by surface:

| Surface | Accepted | Byte cap |
| --- | --- | --- |
| Renderer tree (chat card, Pocket face, face preview) | Bulletin, archive | 2 MiB |
| Chat room and bot icon | inline `data:` | 64 KiB |
| NFT item (purse, purse confirmation, NFT chat message) | Bulletin, via item metadata | 2 MiB |
| Product icon from the manifest | none until the core reads DAGs | none |

The design has four parts:

- **Policy**: Before fetching, the core rejects references not allowed by the surface. All images must be PNG, JPEG, GIF, or WebP, identified by their bytes, within the surface's byte cap, and no larger than 4096 pixels per side. SVG is rejected everywhere.
- **Bulletin**: References use raw blake2b-256 CIDs, which the core decodes into preimage keys and resolves through the existing `lookup_preimage`. Bytes that fail hash verification are discarded, preventing gateways from substituting images. Hosts limit reads to 2 MiB, the maximum size of a Bulletin blob.
- **Archive and inline**: Archive files are loaded through a new optional host callback that receives the product, path, and surface byte cap. Inline `data:` images are decoded directly in the core. Neither source is hash-verified. Chat icon registration applies the same `data:` checks, so an icon that registers passes the same checks as one that draws. `https` icons stay accepted at registration and outside the loader.
- **NFT items**: The core resolves Scarcity metadata in order of precedence: instance, item, then collection. Only the collection owner can write this metadata. The `image` key points to a Bulletin blob, while a Stash item's `manifest` key points to Bulletin JSON containing a picture and thumbnail. Both ultimately follow the Bulletin loading path.

Verified Bulletin bytes are stored in a core cache inaccessible to products and cleared on sign-out. Hosts maintain their disk caches using the content key. iOS and Android implement `lookup_preimage` and the archive callback on the runtime bridge, and replace their Kingfisher and Coil fetchers with `load_image`.

## Compatibility

This introduces breaking changes for products and host app testers. AVIF and non-base64 inline icons fail registration as soon as the core ships. Once hosts adopt the loader, SVG, non-raw-blake2b-256 Bulletin CIDs, and images exceeding the byte caps will no longer render. Existing host code still compiles without changes.

This also amends [unified-renderer.md](unified-renderer.md), where `ImageSource` lists SVG and fetches any CID through the gateway.

## Trade-offs

- Archive and `data:` images are validated but not hash-verified. Products requiring immutable images should use Bulletin.
- Images exceeding a single Bulletin transaction, including manifest product icons, remain unsupported until the core can read DAGs.
- Dropped: HTTP fetching in the core, which has no HTTP client; host-side byte verification; and widening the renderer wire type in place, which would cause deployed hosts to reject entire trees containing unknown variants.
