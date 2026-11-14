---
title: "Materials"
owner: "@valentinfernandez1"
status: draft
---

# RFC — Materials

## Summary

A renderer tree can apply materials to part of itself. A material is a small shader that a provider publishes on
Bulletin in a shared material format, named by the hash of its text. The host fetches each material, checks it,
translates it for its own platform and draws it over that part of the tree, so one material draws the same on every
host. Materials extend the [Unified Renderer](unified-renderer.md), replace its `Effect` node and amend its Closed
requirement.

Tracking issue: [#1758](https://github.com/paritytech/trinity-user-agents/issues/1758), epic
[#1410](https://github.com/paritytech/trinity-user-agents/issues/1410).

## Motivation

A tree can name layouts, theme tokens and images. A designed surface, with foil, relief and highlights that follow the
phone's tilt, cannot be described. The Unified Renderer's `Effect` node has one placeholder effect, `Rainbow`, and one
host-made effect per design would have every host ship every effect, none matching what the designer saw.

Materials already exist outside the hosts: authoring tools define them, and products draw them in their own pages. Hosts
have no shared definition, so the same material would look different in each renderer.

## Requirements

- **Defined.** A material has one definition and one ID, and every host reads it with the same rules.
- **Faithful.** The same material, arguments and inputs draw the same picture in every renderer that follows the format.
- **Bounded.** The work a tree can ask of a host is capped by fixed limits, whatever the product sends.
- **Local.** A material that cannot be drawn affects only its own node, and never breaks the tree or the app.
- **Versioned.** A host never misreads a material or tree newer than itself, and reads every material it ever accepted.

## Approach

### Material

A material is one UTF-8 text file in version 1 of the material format. The format is the
[material format](https://github.com/paritytech/shaderlab-spa/blob/f8251d2bd546b8177a0b6e533552cb3c4e3b363a/docs/materials-platform/MATERIAL_FORMAT.md)
with its
[renderer inputs](https://github.com/paritytech/shaderlab-spa/blob/f8251d2bd546b8177a0b6e533552cb3c4e3b363a/docs/MATERIAL_SPEC.md),
as of commit `f8251d2b`, including the versioning rules it proposes. Its
[conformance vectors](https://github.com/paritytech/shaderlab-spa/tree/f8251d2bd546b8177a0b6e533552cb3c4e3b363a/tools/fixtures/conformance/core)
are the test: a host that disagrees with a vector is wrong. A material's ID is the BLAKE2b-256 hash of its exact
canonical text. The same 32 bytes are its Bulletin preimage key, shown as a CIDv1 with the raw codec in base32.

A provider is any tool or person that publishes a material to Bulletin. A host consumes the format and the ID, and never
depends on which provider published a material.

The format's authors define its new versions, feature names and vectors. Hosts adopt a new version or feature only
through an amendment of this RFC that names it. The format evolves by these rules:

- A published material never changes, and a host reads every format version it ever accepted, forever.
- `// @material N` changes only for a breaking change. A host refuses a version it does not read with
  `unsupported-version`.
- An addition is declared on the `// @requires [...]` line. A host refuses a feature it does not read with
  `unsupported-feature`, and still reads every material that does not require it.
- A host refuses any `// @` directive other than `@material`, `@requires` and `@control`.
- What each renderer input means is fixed per format version. A change of meaning is a new feature or a new version.

Version 1 defines no features.

### Materials node

`Materials` replaces the `Effect` node, and `Effect` with `Rainbow` is removed. `Materials` takes variant index 11, and
index 10, which `Effect` held, is never reused, so a build that still has `Effect` refuses a tree with `Materials`
instead of misreading it.

```rust
/// A node in a product-rendered tree.
pub enum RendererNode {
    // Nil to Image, as in the Unified Renderer.
    /// Draws its children, then runs its layers over them. As large as its
    /// children, which it places at the top start like a `Box`.
    Materials {
        /// The layers to run.
        props: MaterialsProps,
        /// The content the layers draw over.
        children: Vec<RendererNode>,
    },
}

/// Properties of a `Materials` node.
pub struct MaterialsProps {
    /// Layers in drawing order.
    pub layers: Vec<MaterialInstance>,
}

/// One material applied with its arguments.
pub struct MaterialInstance {
    /// Material ID: the BLAKE2b-256 hash of its canonical text.
    pub material: [u8; 32],
    /// One argument per declared parameter, in declaration order.
    pub args: Vec<Parameter>,
}

/// One argument. Numbers are IEEE 754 single-precision bit patterns.
pub enum Parameter {
    /// A `float` parameter.
    Float(u32),
    /// A `vec2` parameter.
    Vec2([u32; 2]),
    /// A `vec3` parameter.
    Vec3([u32; 3]),
    /// A `vec4` parameter.
    Vec4([u32; 4]),
    /// A `samplerFloat` parameter, reading the image's first channel.
    SamplerFloat(ImageRef),
    /// A `samplerVec2` parameter, reading the image's first two channels.
    SamplerVec2(ImageRef),
    /// A `samplerVec3` parameter, reading the image's first three channels.
    SamplerVec3(ImageRef),
    /// A `samplerVec4` parameter, reading all four channels.
    SamplerVec4(ImageRef),
}

/// Where an image argument comes from.
pub enum ImageRef {
    /// A PNG, JPEG or WebP image, loaded like an `Image` node's source.
    Source(ImageSource),
    /// The host's noise image for this body, the same as `r_noise`.
    Noise,
}
```

The base image is the host's drawing of the node's children, clipped to the node's bounds, with what the tree draws
behind the node as the backdrop. Layers chain over it as the renderer inputs define. The last layer's output is drawn in
the node's place, over what is behind it, using its alpha without premultiplication. A node drawn plain shows its
children without its layers.

Any tree may carry `Materials`. A host draws them only on the surfaces it supports, and elsewhere draws the nodes plain
and reports `Unsupported`.

Materials amend the Closed requirement of the Unified Renderer to read:

- **Closed.** A product names layouts and tokens from a fixed vocabulary, never markup, stylesheets or URLs. Materials
  are the one exception: shader code the host runs only inside the node's bounds, with only the inputs the host
  supplies, never able to fetch, read or act on anything else.

### Drawing

Any host may draw any material published on Bulletin, with no approval or library. For each layer the host fetches the
material by ID, cache first, checks it against the ID and the format, and matches the arguments to its parameters by
count, type and finite value. It then translates and compiles the material for its own platform. Image arguments load
like an `Image` node's source, which open RFC [#1800](https://github.com/paritytech/trinity-user-agents/pull/1800) moves
into the core.

A node falls back when one of its materials or images fails: it draws plain, so a product chooses its fallback look by
its children. The rest of the tree draws as usual. While a node's materials or images load, the host draws the node's
last output if it had one, and draws it plain otherwise.

For each node that falls back, the host reports why on `Renderer::failure_subscribe`, once per node per tree, and logs
it on the device with the format's own code where there is one. A product that never subscribes sees no change. Reports
made before the product subscribes are buffered, as actions are. Stepping down for performance is not a failure, and is
not reported.

```rust
/// Product-rendered bodies, the actions triggered inside them, and the
/// materials that fell back in them.
pub trait Renderer: Send + Sync {
    // render and action_subscribe, as in the Unified Renderer.

    /// Subscribe to the `Materials` nodes that fell back in this product's
    /// rendered bodies.
    async fn failure_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostRendererFailureSubscribeItem, CallError<GenericError>> {
        Subscription::interrupted(CallError::unavailable())
    }
}

/// A `Materials` node that fell back.
pub struct HostRendererFailureSubscribeItem {
    /// Where the body lives.
    pub context: RenderContext,
    /// The node's position among the tree's `Materials` nodes, depth first,
    /// starting at 0.
    pub node: u32,
    /// The material that failed, when the failure belongs to one.
    pub material: Option<[u8; 32]>,
    /// Why the node fell back.
    pub reason: MaterialFailure,
}

/// Why a `Materials` node fell back.
pub enum MaterialFailure {
    /// A material or image argument cannot be fetched, or has expired.
    NotFound,
    /// Fetched bytes do not match their ID.
    Integrity,
    /// The material breaks the format, with any format code but
    /// `unsupported-version` and `unsupported-feature`.
    InvalidMaterial,
    /// The arguments do not match the declared parameters, an image is not
    /// PNG, JPEG or WebP, or the noise image fills a parameter other than
    /// `samplerFloat`.
    InvalidStack,
    /// The node breaks a limit.
    LimitExceeded,
    /// The host does not read the version or a required feature, cannot
    /// compile the material, does not draw materials on this surface, or has
    /// materials switched off.
    Unsupported,
}
```

`Unsupported` covers a material the device cannot compile, a surface without materials and materials switched off alike,
so a report does not tell a product which of these happened on the device.

Compiling never blocks drawing or crashes the app. A material that crashed the app is not compiled again on that device.
Materials can be switched off remotely.

The limits are the same on every host:

| Limit                                  | Value |
| -------------------------------------- | ----- |
| Layers in one `Materials` node         | 16    |
| Layers in one tree, across all nodes   | 32    |
| Distinct materials in one tree         | 16    |
| Image arguments per material           | 13    |
| Pixels per side of one image argument  | 2048  |
| Distinct images in one tree, not noise | 16    |
| `Materials` inside another `Materials` | none  |

Every limit but image size is checked on the tree alone, before anything loads, visiting `Materials` nodes depth first.
A node that breaks one falls back with `LimitExceeded` and adds nothing to the per-tree totals. Every other node counts,
whether it draws or falls back later, so a tree falls back at the same nodes on every host. An image wider or taller
than 2048 pixels makes its node fall back with `LimitExceeded` when it loads, and the node still counts. A nested
`Materials` node falls back and its ancestor still draws.

When the materials on screen cost more than the device can afford, the host steps down in this order:

- Stop drawing materials that are not seen.
- Draw a node once, not every frame, when nothing about it changes: no layer reads a time or orientation input, and
  nothing in its children or behind it moves.
- Draw fewer frames.
- Draw fewer pixels, with `sl_resolution` reporting the pixels drawn.
- Freeze: draw one still frame with the reduced-motion inputs.
- Draw every node plain.

The first two steps are always on. A host steps down when the materials on screen take longer than the frame budget, or
the device is hot or in low-power mode. It may start lower on weak devices, and steps back up only after a calm period.
Thresholds and device lists are host policy. Fewer frames and fewer pixels never change what a material means.

### Inputs

Every host supplies the format's renderer inputs, with the same values for the same frame. `sl_time` counts seconds from
zero each time the body comes on screen. `sl_tilt` is `(gamma / 90, beta / 90)`, with beta not clipped.

Device angles are raw: no starting position, gain or smoothing. They use the device's own axes, whatever the screen's
rotation. The platform owns the reference frame, so alpha may be relative to any heading, and a material must not depend
on its absolute value. If beta or gamma is missing, the host keeps the last values. If only alpha is missing,
orientation is zero and rotation is the identity, and tilt carries on. With reduced motion on, or with no motion sensor
the host can read, time and tilt are zero, orientation is zero and rotation is the identity.

The noise image is 1024 × 1024 bytes in row order, made of 32,768 chunks. Chunk `c` is the BLAKE2b-256 hash of the seed
as a little-endian u32 followed by `c` as a little-endian u32. It is sampled nearest with repeat. The seed is the 32-bit
FNV-1a hash of the UTF-8 id that names the body in its `RenderContext`: `card_id`, `message_id` or `candidate_id`.

Images follow the format's rules: channels are normalised, with no colour-space conversion and no premultiplication.
Each image is read as four channels: a greyscale image repeats its value in red, green and blue, and an image without
alpha has alpha 1. The noise image has one channel, so it fills only a `samplerFloat` parameter. Images sample linearly
with clamped edges, the noise image nearest with repeat. Images between layers keep 32-bit float precision. Where a GPU
cannot sample them linearly, the host filters them in the shader.

### Capability

A product asks before it sends a material of a given format version and features. `System::feature_supported` learns one
question.

```rust
/// Request to query whether a feature is supported by the host.
pub enum HostFeatureSupportedRequest {
    // Chain, as today.
    /// Ask whether the host draws materials of one format version, requiring
    /// these features.
    Materials {
        /// Format version, as on the `// @material` line.
        format_version: u32,
        /// Feature names, as on the `// @requires` line.
        features: Vec<String>,
    },
}
```

The answer is yes only when the host reads that version and every named feature, and has materials switched on. A host
that answers yes serves `Renderer::failure_subscribe`.

## Compatibility

- Removing `Effect` breaks every tree that uses it. Hosts and products move to `Materials` together.
- Products ask the capability question before sending materials.
- Before version 1 is adopted, the format's reference parser and the host's validation both refuse unknown directives
  and read `// @requires`, and the format's vectors cover those rules and the seed.

## Trade-offs

- Any product can make the host run shader code it wrote. The safety rules, the limits and the step-down order are what
  that costs every host.
- Hosts translate for themselves, so agreement between hosts rests on the format's vectors.
- Limits are fixed, so raising one is an amendment.
- Out of scope: a material library and users' own materials, DotNS names for materials, and renewing expired materials,
  which open RFC [#1833](https://github.com/paritytech/trinity-user-agents/pull/1833) covers. A product's own pages draw
  materials themselves, and fetch them through the existing preimage service.
- Considered and dropped:
  - Material text inline in the tree. A 13-layer stack is 1,694 bytes by ID and 60,857 inline, and a tree is resent on
    every change.
  - A `capabilities()` call listing versions, features and limits. The limits are fixed, and one question covers the
    rest.
  - The node's own text or image shape as a host image. It depends on an editor's own objects.
  - Keeping `Effect` beside `Materials`. Its only effect is a placeholder, and materials cover what it was for.
