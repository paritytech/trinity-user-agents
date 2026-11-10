// Bakes the studio into the cube the coin shader samples, on the device. A port of the reference's
// scripts/lib/env-bake.mjs over src/coin/studio-radiance.js: the same lamps, the same room, the
// same GGX prefilter and sample sequence, so the cube matches the one the files used to carry.
//
// Cube face convention, shared with the renderer's sampler: faces +X, −X, +Y, −Y, +Z, −Z; texel
// (column i, row j) of a size×size face maps to s = 2(i+½)/size − 1, t = 2(j+½)/size − 1 and
//   +X ( 1, −t, −s)   −X (−1, −t,  s)
//   +Y ( s,  1,  t)   −Y ( s, −1, −t)
//   +Z ( s, −t,  1)   −Z (−s, −t, −1)
//
// Level L of N is prefiltered for perceptual roughness r = L/(N−1), GGX with α = r², under the
// split-sum assumption N = V = R, so the coin shader samples level roughness·(N−1).
#include <metal_stdlib>
using namespace metal;

/// One softbox, as the exporter preprocessed it: a rectangle facing the origin.
struct StudioLamp {
  packed_float3 center;
  packed_float3 uAxis;
  packed_float3 vAxis;
  packed_float3 normal;
  float halfU;
  float halfV;
  packed_float3 tint;
  float intensity;
  float roomHere;
};

struct StudioBake {
  float roomLow;
  float roomHigh;
  uint lampCount;
  uint sampleCount;
};

static float3 faceDirection(uint face, float s, float t) {
  switch (face) {
    case 0: return float3(1.0, -t, -s);
    case 1: return float3(-1.0, -t, s);
    case 2: return float3(s, 1.0, t);
    case 3: return float3(s, -1.0, -t);
    case 4: return float3(s, -t, 1.0);
    default: return float3(-s, -t, -1.0);
  }
}

/// Radiance arriving at the origin from the unit direction `d`: the nearest lamp the ray meets
/// from the front, falling off toward its edges, else the room's gradient from floor to ceiling.
static float3 radiance(float3 d, constant StudioLamp *lamps, constant StudioBake &bake) {
  float best = INFINITY;
  int hit = -1;
  float hu = 0.0;
  float hv = 0.0;

  for (uint i = 0; i < bake.lampCount; ++i) {
    constant StudioLamp &lamp = lamps[i];
    float3 normal = float3(lamp.normal);
    float denom = dot(d, normal);
    if (denom >= -1e-6) { continue; }
    float3 center = float3(lamp.center);
    float t = dot(center, normal) / denom;
    if (t <= 0.0 || t >= best) { continue; }
    float3 p = d * t - center;
    float lu = dot(p, float3(lamp.uAxis)) / lamp.halfU;
    float lv = dot(p, float3(lamp.vAxis)) / lamp.halfV;
    if (abs(lu) > 1.0 || abs(lv) > 1.0) { continue; }
    best = t;
    hit = int(i);
    hu = lu;
    hv = lv;
  }

  if (hit >= 0) {
    constant StudioLamp &lamp = lamps[hit];
    float f = lamp.intensity * (1.0 - 0.12 * (hu * hu + hv * hv));
    return float3(lamp.tint) * f + lamp.roomHere;
  }

  float g = bake.roomLow + (bake.roomHigh - bake.roomLow) * smoothstep(-0.35, 0.85, d.y);
  return float3(g);
}

/// Level 0: the studio itself, two by two supersampled.
kernel void studioSharp(
    texture2d_array<half, access::write> level [[texture(0)]],
    constant StudioLamp *lamps [[buffer(0)]],
    constant StudioBake &bake [[buffer(1)]],
    uint3 gid [[thread_position_in_grid]]) {
  uint size = level.get_width();
  if (gid.x >= size || gid.y >= size) { return; }
  float n = float(size);
  float3 sum = float3(0.0);
  for (int sy = 0; sy < 2; ++sy) {
    for (int sx = 0; sx < 2; ++sx) {
      float s = 2.0 * (float(gid.x) + (float(sx) + 0.5) / 2.0) / n - 1.0;
      float t = 2.0 * (float(gid.y) + (float(sy) + 0.5) / 2.0) / n - 1.0;
      sum += radiance(normalize(faceDirection(gid.z, s, t)), lamps, bake);
    }
  }
  level.write(half4(half3(sum * 0.25), 1.0h), gid.xy, gid.z);
}

/// Levels 1 and up: the studio seen through a GGX lobe of the level's roughness, from the sample
/// directions the host drew for it, each weighted by its cosine as in the reference.
kernel void studioPrefilter(
    texture2d_array<half, access::write> level [[texture(0)]],
    constant StudioLamp *lamps [[buffer(0)]],
    constant StudioBake &bake [[buffer(1)]],
    constant packed_float3 *samples [[buffer(2)]],
    uint3 gid [[thread_position_in_grid]]) {
  uint size = level.get_width();
  if (gid.x >= size || gid.y >= size) { return; }
  float n = float(size);
  float s = 2.0 * (float(gid.x) + 0.5) / n - 1.0;
  float t = 2.0 * (float(gid.y) + 0.5) / n - 1.0;
  float3 N = normalize(faceDirection(gid.z, s, t));
  float3 up = abs(N.z) < 0.999 ? float3(0.0, 0.0, 1.0) : float3(1.0, 0.0, 0.0);
  float3 T = normalize(cross(up, N));
  float3 B = cross(N, T);

  float3 sum = float3(0.0);
  float weight = 0.0;
  for (uint k = 0; k < bake.sampleCount; ++k) {
    float3 h = float3(samples[k]);
    float3 H = T * h.x + B * h.y + N * h.z;
    float3 L = 2.0 * h.z * H - N;
    float nl = dot(L, N);
    if (nl <= 0.0) { continue; }
    sum += radiance(L, lamps, bake) * nl;
    weight += nl;
  }
  level.write(half4(half3(sum / max(weight, 1e-6)), 1.0h), gid.xy, gid.z);
}
