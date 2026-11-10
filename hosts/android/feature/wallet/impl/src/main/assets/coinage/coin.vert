#version 300 es
// Coin vertex shader, GLSL ES 3.0. Line-by-line port of CoinageCoin.metal's `coinVertex`, which is
// itself a port of the reference's native/shaders/coin.vert; section names match.
//
// Differences from the Metal version, all mechanical:
//   - GL NDC depth is -1..1 (Metal: 0..1), so z maps to -z/1000 rather than 0.5 - z/2000.
//   - Flat varyings use `flat out`; instance attributes come from a divisor-1 buffer.
//   - GL's default front face is already counter-clockwise, so the host leaves it alone.

layout(location = 0) in vec3 aPosition;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec4 aSurf;   // zone, edge u, wall weight, field weight
layout(location = 3) in vec4 iPos;    // per instance: x, y (world, points, y up), z, height
layout(location = 4) in vec4 iRot;    // turn, tilt, spin, thickness scale
layout(location = 5) in vec4 iLook;   // wear, outer metal, core metal or -1, relief tile
layout(location = 6) in vec4 iFx;     // reeds, luster, recede, edge calm
layout(location = 7) in vec4 iMarks;  // pits (hops and splits), streaks, seed, spare

uniform vec2 uViewport;

out vec3 vPos;
out vec3 vNormal;
out vec4 vSurf;
out vec2 vWorldXY;
flat out vec4 vPosI;
flat out vec4 vRotI;
flat out vec4 vLookI;
flat out vec4 vFxI;
flat out vec4 vMarksI;
flat out vec3 vAxisX;
flat out vec3 vAxisY;
flat out vec3 vAxisZ;

vec3 rotateCoin(vec3 v, vec3 a) {
  float cz = cos(a.z), sz = sin(a.z);
  vec3 v1 = vec3(v.x * cz - v.y * sz, v.x * sz + v.y * cz, v.z);
  float cx = cos(a.y), sx = sin(a.y);
  vec3 v2 = vec3(v1.x, v1.y * cx - v1.z * sx, v1.y * sx + v1.z * cx);
  float cy = cos(a.x), sy = sin(a.x);
  return vec3(v2.x * cy + v2.z * sy, v2.y, v2.z * cy - v2.x * sy);
}

void main() {
  vec3 scale = vec3(iPos.w, iPos.w, iPos.w * iRot.w);
  vec3 world = iPos.xyz + rotateCoin(aPosition * scale, iRot.xyz);
  gl_Position = vec4(
    world.x / uViewport.x * 2.0 - 1.0,
    world.y / uViewport.y * 2.0 + 1.0,
    -world.z / 1000.0,
    1.0
  );
  vPos = aPosition;
  vNormal = aNormal;
  vSurf = aSurf;
  vWorldXY = world.xy;
  vPosI = iPos;
  vRotI = iRot;
  vLookI = iLook;
  vFxI = iFx;
  vMarksI = iMarks;
  vAxisX = rotateCoin(vec3(1.0, 0.0, 0.0), iRot.xyz);
  vAxisY = rotateCoin(vec3(0.0, 1.0, 0.0), iRot.xyz);
  vAxisZ = rotateCoin(vec3(0.0, 0.0, 1.0), iRot.xyz);
}
