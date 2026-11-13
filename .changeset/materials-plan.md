---
"@parity/truapi-host": minor
---

Native hosts can plan the materials of any tree: `observeMaterialPlan` on the runtime and on a product execution reports, for each `Materials` node, whether it is loading, ready with its shaders for the platform (Metal on iOS, GLSL ES elsewhere), or drawn plain and why, and takes the app's reports of shaders or images it could not use. `materialNoise` and `checkMaterialImage` go with it. Products now get `Renderer::failureSubscribe`, and the materials question answers yes when the host sets `HostRuntimeConfig.materialsEnabled`, which is off by default, so hosts change nothing until they turn it on.
