---
"@parity/truapi-host": patch
---

Native hosts build each material's shaders: the GLSL ES 3.00 an OpenGL ES app compiles, the same shaders Shaderlab's own compile step writes, and their Metal translation through naga, with where each uniform and texture goes. Nothing calls this yet, and hosts change nothing.
