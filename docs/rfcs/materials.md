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

A material is one UTF-8 text file in version 1 of the material format. The format is the one Shaderlab's
[reference parser](https://github.com/paritytech/shaderlab-spa/blob/66dd4552edea41787c10469b3088781f6e8f4395/packages/material-core/src/format.ts)
reads, with its
[renderer inputs](https://github.com/paritytech/shaderlab-spa/blob/66dd4552edea41787c10469b3088781f6e8f4395/docs/MATERIAL_SPEC.md),
as of commit `66dd4552` on Shaderlab's `main`, including its versioning rules. Its
[conformance vectors](https://github.com/paritytech/shaderlab-spa/tree/66dd4552edea41787c10469b3088781f6e8f4395/tools/fixtures/conformance/core)
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
    /// How far past the node the layers draw, in logical pixels on every side,
    /// at most 64. Zero when absent from JSON.
    pub bleed: Size,
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

A node with a `bleed` draws past its bounds, so an effect can leave the card it sits in. Its stage is the node's bounds
grown by `bleed` on every side: the base image is that large, with the children at the inset and the margin transparent,
the shader's `uv` and `sl_resolution` cover the whole stage, and the last layer's output is drawn over the rest of the
tree, past the clip of every node around it. The layout does not change: the node is as large as its children, and the
margin covers its neighbours without taking their touch.

Any tree may carry `Materials`. A host draws them only on the surfaces it supports, and elsewhere draws the nodes plain
and reports `Unsupported`.

Materials amend the Closed requirement of the Unified Renderer to read:

- **Closed.** A product names layouts and tokens from a fixed vocabulary, never markup, stylesheets or URLs. Materials
  are the one exception: shader code the host runs only inside the node's bounds and its bleed, with only the inputs the
  host supplies, never able to fetch, read or act on anything else.

### Drawing

Any host may draw any material published on Bulletin, with no approval or library. For each layer the host fetches the
material by ID, cache first, checks it against the ID and the format, and matches the arguments to its parameters by
count, type and finite value. It then translates and compiles the material for its own platform. Image arguments load
like an `Image` node's source, which open RFC [#1800](https://github.com/paritytech/trinity-user-agents/pull/1800) moves
into the core.

A node falls back when one of its materials or images fails: it draws plain, so a product chooses its fallback look by
its children. The rest of the tree draws as usual. While a node's materials or images load, the host draws the node's
last output if it had one, and draws it plain otherwise. A new tree of the same body counts as loading: until a node's
layers are ready for it, the host goes on drawing the last output of a node of the tree before with the same materials
in the same order.

For each node that falls back, the host reports why on `Renderer::failure_subscribe`, once per node per tree, and logs
it on the device with the format's own code where there is one. A product that never subscribes sees no change. Reports
made before the product subscribes are buffered, as actions are. Stepping down for performance is not a failure, and is
not reported. The request carries nothing and the error is `GenericError`, each in the versioned envelope every method
has.

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
        _request: HostRendererFailureSubscribeRequest,
    ) -> Subscription<HostRendererFailureSubscribeItem, CallError<HostRendererFailureSubscribeError>> {
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
| `bleed` of one `Materials` node        | 64    |
| `Touch` nodes in one tree              | 4     |
| `Touch` inside another `Touch`         | none  |
| Triggers in one `Touch` node           | 8     |

Every limit but image size is checked on the tree alone, before anything loads, visiting `Materials` and `Touch` nodes
depth first. A `TrailCovers` trigger in a node with no trail, or a region past 1000 per mille, breaks the node's limits
too. A node that breaks one falls back with `LimitExceeded` and adds nothing to the per-tree totals. Every other node
counts, whether it draws or falls back later, so a tree falls back at the same nodes on every host. An image wider or
taller than 2048 pixels makes its node fall back with `LimitExceeded` when it loads, and the node still counts. A nested
`Materials` node falls back and its ancestor still draws.

When the materials on screen cost more than the device can afford, the host steps down in this order:

- Stop drawing materials that are not seen.
- Draw a node once, not every frame, when nothing about it changes: no layer reads a time or orientation input, no layer
  takes a touch argument that changed, and nothing in its children or behind it moves.
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
FNV-1a hash of the UTF-8 id that names the body in its `RenderContext`: `card_id`, `message_id`, `candidate_id`, or
`room_id` for a room's footer.

Images follow the format's rules: channels are normalised, with no colour-space conversion and no premultiplication.
Each image is read as four channels: a greyscale image repeats its value in red, green and blue, and an image without
alpha has alpha 1. The noise image has one channel, so it fills only a `samplerFloat` parameter. Images sample linearly
with clamped edges, the noise image nearest with repeat. Images between layers keep 32-bit float precision. Where a GPU
cannot sample them linearly, the host filters them in the shader.

### Touch

A `Touch` node takes the finger over its bounds. It feeds the finger to the `Materials` nodes inside it through
arguments the host fills, and tells the product what happened through triggers. The material format does not change: the
host fills a declared parameter itself, as it fills one with the noise image. A material author previews in Shaderlab by
moving that parameter by hand. `Touch` takes variant index 13.

```rust
pub enum RendererNode {
    // Nil to Materials, as above.
    /// Takes the finger over its bounds and feeds it to the `Materials` nodes
    /// inside. As large as its children, which it places at the top start
    /// like a `Box`.
    Touch { props: TouchProps, children: Vec<RendererNode> },
}

/// Properties of a `Touch` node.
pub struct TouchProps {
    /// Which finger movement the node keeps from the scroll around it.
    pub drag: DragAxis,
    /// The trail the finger leaves, when the node keeps one.
    pub trail: Option<TrailProps>,
    /// What the host tells the product, at most 8.
    pub triggers: Vec<TouchTrigger>,
    /// The clock a `Replay` argument reads, when the node keeps one.
    pub replay: Option<ReplayProps>,
}

pub enum DragAxis {
    /// A drag mostly sideways. A drag mostly up or down scrolls.
    Horizontal,
    /// Every drag that starts inside the node.
    Any,
}

pub struct TrailProps {
    /// Brush radius in logical pixels.
    pub radius: Size,
}

/// The replay clock: it starts when the host first draws the node, and starts
/// again when a finger goes down once it has run `length`.
pub struct ReplayProps {
    /// How long the story plays, in milliseconds.
    pub length: u32,
}

pub struct TouchTrigger {
    pub when: TouchCondition,
    /// Sent with an empty payload on `Renderer::action_subscribe`.
    pub action: String,
}

pub enum TouchCondition {
    /// Down and up inside the region, having moved under 10 logical pixels.
    /// Fires every time.
    Tap(Region),
    /// Down inside `from`, then inside `to` before lifting. Fires once per tree.
    Drag { from: Region, to: Region },
    /// At least this many per mille of the trail cells are marked. Fires once
    /// per tree.
    TrailCovers(u16),
}

/// A rectangle in per mille of the node, from its top left.
pub struct Region { pub x: u16, pub y: u16, pub width: u16, pub height: u16 }

pub enum Parameter {
    // Float to SamplerVec4, as above.
    /// Fills a `vec4`: the finger's last position, then where that touch went down.
    Touch,
    /// Fills a `vec2`: 1 while the finger is down and 0 otherwise, then seconds
    /// since it last went down or lifted.
    TouchState,
    /// Fills a `float`: seconds since the host first drew this tree.
    Elapsed,
    /// Fills a `float`: seconds since the replay clock of the enclosing
    /// `Touch` node last started.
    Replay,
}

pub enum ImageRef {
    // Source and Noise, as above.
    /// The trail of the enclosing `Touch` node. Fills only `samplerFloat`.
    Trail,
}
```

The values are the same on every host:

- Positions are in the `Touch` node's unit square, `(0, 0)` at the top left and `(1, 1)` at the bottom right, clamped.
  In the usual tree, a `Touch` node around one `Materials` node, this equals the shader's `uv`.
- Before any touch, `Touch` is `(0, 0, 0, 0)` and `TouchState` is `(0, -1)`. After a lift the last position stays.
- `Elapsed` keeps counting while the body is off screen, and restarts when the product renders a new tree for the body.
  With reduced motion on it reads 3600, so an entry animation shows its end. A host restarts it together with the
  drawing that carries the new tree's arguments, so no frame shows the new clock over the old arguments.
- `Replay` plays a story once from the first draw and again on a tap, ignoring taps until the story has played out. The
  clock starts when the host first draws the `Touch` node. A finger that goes down inside the node more than `length`
  after the clock last started starts it again at that moment, wherever and whenever it lifts. A finger that goes down
  sooner leaves it running. With reduced motion on it reads 3600, as `Elapsed` does.
- The trail has one byte per cell, in row order from the top left. The longer side has 128 cells, the other
  `max(1, round(128 × short / long))`. A cell becomes 255 when its centre lies within `radius` of the segment between
  two consecutive finger samples of one touch, or of the first sample. Cells never clear. The trail samples linearly
  with clamped edges. `TrailCovers(p)` holds when `marked × 1000 >= p × cells`.
- The trail, the fired triggers, the last touch and the replay clock last as long as the app shows the body and the
  node's `TouchProps` stay equal across new trees, counting `Touch` nodes depth first. A tree resent with equal props
  therefore does not replay the story. Different props or another node size start them empty. They do not outlive the
  app, so a product keeps a result by sending the tree that shows it.
- A layer with a `TouchState`, `Elapsed` or `Replay` argument draws every frame while on screen. A layer with only
  `Touch` or `Trail` draws when the finger moves.
- Touch works with reduced motion on.

A `Touch` node that breaks a limit draws its children like a `Box` and takes no touch. A `Materials` node with a
`Touch`, `TouchState`, `Replay` or `Trail` argument and no `Touch` node around it that takes touch, with `Trail` where
that node keeps no trail, or with `Replay` where it keeps no replay clock, falls back with `InvalidStack`. The nearest
`Touch` node around it decides. `Trail` in a sampler other than `samplerFloat` falls back with `InvalidStack`, as the
noise image does.

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

A second question asks whether the host draws `Touch` nodes and fills the touch arguments. The core answers it beside
the materials question. An older host cannot decode it, and the product reads that error as no, then sends a tree with a
button instead.

```rust
pub enum HostFeatureSupportedRequest {
    // Chain and Materials, as above.
    /// Ask whether the host draws `Touch` nodes and fills the touch arguments.
    RendererTouch,
}
```

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
