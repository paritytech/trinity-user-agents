// Coin shaders, Metal Shading Language (iOS/iPadOS/macOS). Line-by-line
// port of native/shaders/coin.vert + coin.frag (themselves ports of
// src/coin/coin-material.js); section names match. native/README.md
// documents every input; native/reference/metal/render.swift is a complete
// host program using these functions.
//
// Differences from the GLSL version, all mechanical:
//   - NDC depth is 0..1 (GL: −1..1), so z maps to 0.5 − z/2000.
//   - Flat varyings use [[flat]]; texture sampling uses level(lod).
//   - Front faces are counter-clockwise: the host must call
//     setFrontFacing(.counterClockwise) (Metal's default is clockwise).
#include <metal_stdlib>
using namespace metal;

// Scalars in the order of CoinParams in render.swift (all float, no padding).
struct CoinParams {
  float2 viewport;       // surface size in points
  float dpr;             // device pixels per point
  float tilePx;          // atlas tile size (512)
  float envMaxLod;       // cube levels − 1 (6)
  float lightTurnX;      // studio turned with the phone: an axis scaled by the angle turned
  float lightTurnY;      // about it, so a wrist roll and a lifted edge stay different turns
  float lightTurnZ;
  float exposure, luster, rimLuster, lusterMinPx, studioRadius, studioScale;
  float basin, rollGloss, rollReliefHaze, envSharpen, haze, polish, tone, grime;
  float engraveDark, frost, wallRough, aaVariance, aaThreshold, reliefBias;
  float backdropR, backdropG, backdropB;
  int debug;
};

struct VertexIn {
  float3 position [[attribute(0)]];
  float3 normal   [[attribute(1)]];
  float4 surf     [[attribute(2)]];  // zone, edge u, wall weight, field weight
  float4 iPos     [[attribute(3)]];  // per instance: x, y (world, points, y up), z, height
  float4 iRot     [[attribute(4)]];  // turn, tilt, spin, thickness scale
  float4 iLook    [[attribute(5)]];  // wear, outer metal, core metal or −1, relief tile
  float4 iFx      [[attribute(6)]];  // reeds, luster, recede, edge calm
  float4 iMarks   [[attribute(7)]];  // pits (hops and splits), streaks, seed, spare
};

struct VertexOut {
  float4 clip [[position]];
  float3 pos;
  float3 normal;
  float4 surf;
  float2 worldXY;
  float4 posI  [[flat]];
  float4 rotI  [[flat]];
  float4 lookI [[flat]];
  float4 fxI   [[flat]];
  float4 marksI [[flat]];
  float3 axisX [[flat]];
  float3 axisY [[flat]];
  float3 axisZ [[flat]];
};

static float3 rotateCoin(float3 v, float3 a) {
  float cz = cos(a.z), sz = sin(a.z);
  float3 v1 = float3(v.x * cz - v.y * sz, v.x * sz + v.y * cz, v.z);
  float cx = cos(a.y), sx = sin(a.y);
  float3 v2 = float3(v1.x, v1.y * cx - v1.z * sx, v1.y * sx + v1.z * cx);
  float cy = cos(a.x), sy = sin(a.x);
  return float3(v2.x * cy + v2.z * sy, v2.y, v2.z * cy - v2.x * sy);
}

vertex VertexOut coinVertex(VertexIn in [[stage_in]], constant CoinParams &P [[buffer(4)]]) {
  VertexOut o;
  float3 scale = float3(in.iPos.w, in.iPos.w, in.iPos.w * in.iRot.w);
  float3 world = in.iPos.xyz + rotateCoin(in.position * scale, in.iRot.xyz);
  o.clip = float4(world.x / P.viewport.x * 2.0 - 1.0, world.y / P.viewport.y * 2.0 + 1.0, 0.5 - world.z / 2000.0, 1.0);
  o.pos = in.position;
  o.normal = in.normal;
  o.surf = in.surf;
  o.worldXY = world.xy;
  o.posI = in.iPos;
  o.rotI = in.iRot;
  o.lookI = in.iLook;
  o.fxI = in.iFx;
  o.marksI = in.iMarks;
  o.axisX = rotateCoin(float3(1, 0, 0), in.iRot.xyz);
  o.axisY = rotateCoin(float3(0, 1, 0), in.iRot.xyz);
  o.axisZ = rotateCoin(float3(0, 0, 1), in.iRot.xyz);
  return o;
}

constant float ATLAS_TILES = 4.0;
constant float REVERSE_TILE = 15.0;
constant float RELIEF_RADIUS = 0.37;
constant float PI_F = 3.14159265358979;
constant float LUSTER_TAPS[6] = { -1.25, -0.75, -0.25, 0.25, 0.75, 1.25 };

static float3 toWorld(VertexOut in, float3 v) { return in.axisX * v.x + in.axisY * v.y + in.axisZ * v.z; }

// Wear marks. Two things happen to a coin and they are deliberately different to look at.
//
// Every payment a coin has been through leaves a pit: a struck dish, few and large, that reads as
// damage done TO the coin. How far its recycler still has to go leaves streaks: many fine scratches
// that read as a dull, scuffed surface. A coin nobody can follow is clean of both.
//
// Both are procedural and seeded per coin, so a coin keeps its own face across frames without any
// storage. Both are pushed harder than the reference's own wear, which is legible on a large render
// and all but invisible at strip size.
constant float MARK_INNER = 0.30;   // clear of the struck figure, which has to stay readable
constant float MARK_OUTER = 0.45;   // out to the rim
constant float PIT_RADIUS = 0.10;   // a pit is large on purpose: you can count them
constant float PIT_DEPTH = 2.6;
constant float PIT_FLOOR = 0.51;    // how dark the bottom of a pit goes; 1.0 would not darken at all
constant float STREAK_DEPTH = 1.65;
constant int PIT_MAX = 8;
constant int STREAK_MAX = 12;

// How much harder than the reference a coin wears. Its own figure is tuned for a coin filling a
// desktop window; here the largest is sixty points tall, and the four things wear does — toning the
// metal toward its own darker cast, hazing the field while the relief rubs shiny, killing the mint
// luster, and collecting grime in the cut figure — all have to read at that size. Saturates early
// rather than reaching further: a fully worn coin looked right, everything between did not.
constant float WEAR_GAIN = 1.5;

static float2 markHash(float seed) {
  return fract(sin(float2(seed * 12.9898, seed * 78.233 + 1.7)) * float2(43758.5453, 22578.1459));
}

// Pits: circular dishes in the field. The slope runs outward from the centre, which under a key
// light from the upper left gives a dark upper wall and a lit lower lip, the way a strike reads.
static void addPits(float2 local, float count, float seed, thread float2 &slope, thread float &shade) {
  int total = min(int(count + 0.5), PIT_MAX);

  for (int i = 0; i < total; ++i) {
    float2 h = markHash(seed + float(i) * 7.13);
    float angle = h.x * 2.0 * PI_F;
    float ring = mix(MARK_INNER, MARK_OUTER, h.y);
    float2 delta = local - float2(cos(angle), sin(angle)) * ring;
    float dist = length(delta) / PIT_RADIUS;

    if (dist >= 1.0) { continue; }

    float bowl = 1.0 - dist * dist;
    slope += normalize(delta + float2(1e-5, 0.0)) * PIT_DEPTH * dist * bowl;
    shade *= mix(1.0, PIT_FLOOR, bowl * bowl);
  }
}

// Streaks: short grooves at random angles, as many as the coin is still traceable. They fade out
// over the struck figure, so a heavily scuffed coin still reads its own value.
static void addStreaks(float2 local, float amount, float seed, thread float2 &slope, thread float &rough) {
  int total = min(int(amount * float(STREAK_MAX) + 0.5), STREAK_MAX);
  float clear = smoothstep(MARK_INNER * 0.72, MARK_INNER, length(local));

  if (clear <= 0.0) { return; }

  for (int i = 0; i < total; ++i) {
    float2 h = markHash(seed + 31.7 + float(i) * 3.77);
    float2 g = markHash(seed + 91.3 + float(i) * 5.21);
    float angle = h.x * PI_F;
    float2 along = float2(cos(angle), sin(angle));
    float2 across = float2(-along.y, along.x);
    float2 centre = (g - 0.5) * 2.0 * MARK_OUTER;
    float2 delta = local - centre;
    float gap = abs(dot(delta, across));
    float run = abs(dot(delta, along));
    float reach = mix(0.10, 0.26, h.y);
    float width = 0.008 + 0.012 * g.x;

    if (gap >= width || run >= reach) { continue; }

    float groove = (1.0 - gap / width) * (1.0 - run / reach) * clear;
    slope += across * sign(dot(delta, across)) * STREAK_DEPTH * groove;
    rough += 0.3 * groove;
  }
}

// Turns the studio with the phone, about the axis the phone actually turned about. Replaces
// upstream's yawDir, which this generalises: a turn about y is the case where the axis is y.
static float3 tiltDir(float3 d, float3 turn) {
  float angle = length(turn);
  if (angle < 1e-6) return d;
  float3 axis = turn / angle;
  float c = cos(angle), s = sin(angle);
  return d * c + cross(axis, d) * s + axis * dot(axis, d) * (1.0 - c);
}

static float3 lightTurn(constant CoinParams &P) {
  return float3(P.lightTurnX, P.lightTurnY, P.lightTurnZ);
}

static float3 envBRDFApprox(float3 f0, float rough, float ndv) {
  float4 r = float4(-1.0, -0.0275, -0.572, 0.022) * rough + float4(1.0, 0.0425, 1.04, -0.04);
  float a004 = min(r.x * r.x, exp2(-9.28 * ndv)) * r.x + r.y;
  float2 ab = float2(-1.04, 1.04) * a004 + r.zw;
  return f0 * ab.x + ab.y;
}

static float3 neutralShoulder(float3 c) {
  const float start = 0.76;
  const float d = 1.0 - start;
  float peak = max(max(c.r, c.g), max(c.b, 1e-6));
  if (peak <= start) return c;
  float newPeak = 1.0 - d * d / (peak + d - start);
  c *= newPeak / peak;
  float g = 1.0 - 1.0 / ((peak - newPeak) * 0.15 + 1.0);
  return mix(c, float3(newPeak), g);
}

static float3 srgbEncode(float3 c) {
  float3 lo = c * 12.92;
  float3 hi = pow(max(c, float3(0.0)), float3(1.0 / 2.4)) * 1.055 - 0.055;
  return select(hi, lo, c <= float3(0.0031308));
}

static float3 envAt(texturecube<float> env, sampler s, constant CoinParams &P, float3 p, float pp, float3 Rw, float r) {
  float3 R = tiltDir(Rw, lightTurn(P));
  float b = dot(p, R);
  float t = -b + sqrt(max(b * b - pp, 0.0));
  float3 dir = normalize(p + R * t);
  float lod = clamp(r * P.envSharpen, 0.0, 1.0) * P.envMaxLod;
  return env.sample(s, dir, level(lod)).rgb;
}

fragment float4 coinFragment(VertexOut in [[stage_in]],
                             constant CoinParams &P [[buffer(4)]],
                             constant float4 *metals [[buffer(5)]],
                             texture2d<float> atlas [[texture(0)]],
                             texturecube<float> env [[texture(1)]],
                             sampler linearMip [[sampler(0)]]) {
  float3 lp = in.pos;
  float thick = in.rotI.w;
  float3 n = normalize(in.normal / float3(1.0, 1.0, max(thick, 1e-3)));
  float px = in.posI.w * P.dpr;

  // ---- relief ----
  bool front = lp.z > 0.0;
  float side = front ? 1.0 : -1.0;
  float tile = front ? in.lookI.w : REVERSE_TILE;
  float2 tileXY = float2(fmod(tile, ATLAS_TILES), floor(tile / ATLAS_TILES));
  float2 local = float2(lp.x * side, lp.y);
  float2 uv = (tileXY + float2(local.x + 0.5, 0.5 - local.y)) / ATLAS_TILES;
  float lod = clamp(log2(P.tilePx / max(px, 1.0)) + P.reliefBias, 0.0, 7.0);
  float4 texel = atlas.sample(linearMip, uv, level(lod));
  float3 nr = texel.rgb * 2.0 - 1.0;
  float nrLen = max(length(nr), 1e-3);
  float height = texel.a * 2.0 - 1.0;
  float mask = in.surf.w * (1.0 - smoothstep(RELIEF_RADIUS - 0.015, RELIEF_RADIUS + 0.01, length(lp.xy)));
  float2 slope = float2(nr.x * side, nr.y) / max(nr.z, 0.2);
  float rr = length(lp.xy);
  float k = PI_F / 2.0 / RELIEF_RADIUS;
  float basinSlope = P.basin * k * sin(rr * 2.0 * k) * (rr < RELIEF_RADIUS ? 1.0 : 0.0);
  slope -= lp.xy / max(rr, 1e-4) * basinSlope;
  // ---- wear marks ----
  float2 markSlope = float2(0.0);
  float markShade = 1.0;
  float markRough = 0.0;

  if (front && in.surf.w > 0.5) {
    addPits(local, in.marksI.x, in.marksI.z, markSlope, markShade);
    addStreaks(local, in.marksI.y, in.marksI.z, markSlope, markRough);
  }

  slope += markSlope;
  n = normalize(n + float3(slope, 0.0) * mask);
  float reliefVar = max((1.0 - nrLen) / nrLen, 0.0) * mask;
  float cut = clamp((-height - 0.05) * 1.3, 0.0, 1.0) * mask;
  float rimTop = (1.0 - in.surf.w) * (1.0 - in.surf.z);
  float high = max(clamp(height * 1.6, 0.0, 1.0) * mask, rimTop * 0.8);

  // ---- edge calm ----
  float calm = clamp(in.fxI.w, 0.0, 1.0) * in.surf.z;
  float3 radial = normalize(float3(lp.x, lp.y, 0.0) + float3(1e-5, 0.0, 0.0));
  n = normalize(mix(n, float3(radial.x, radial.y, n.z), calm));

  // ---- reeded edge ----
  float reeds = in.fxI.x * (1.0 - calm);
  float phase = in.surf.y * reeds;
  float pxPerReed = px * PI_F / max(reeds, 1.0);
  float reedOn = smoothstep(2.2, 4.5, pxPerReed) * in.surf.z * (reeds > 0.5 ? 1.0 : 0.0);
  float3 tangent = float3(-n.y, n.x, 0.0);
  n = normalize(n + tangent * (sin(phase * 2.0 * PI_F) * 0.6 * reedOn));
  float reedAO = mix(1.0, 0.62, (cos(phase * 2.0 * PI_F) * 0.5 + 0.5) * reedOn);

  // ---- standard inputs ----
  bool isCore = in.surf.x > 0.5 && in.lookI.z > -0.5;
  int metalIndex = int(isCore ? in.lookI.z : in.lookI.y);
  float4 m0 = metals[metalIndex * 2];
  float4 m1 = metals[metalIndex * 2 + 1];
  float wear = min(clamp(in.lookI.x, 0.0, 1.0) * WEAR_GAIN, 1.0);
  float toning = wear * P.tone * mix(0.55, 1.0, cut);
  float3 baseColor = m0.xyz * mix(float3(1.0), m1.xyz, toning);
  float inRoll = 1.0 - clamp(in.fxI.y, 0.0, 1.0);
  float rough = clamp(m0.w - inRoll * P.rollGloss + in.surf.z * P.wallRough + cut * P.frost
                      + wear * P.haze * (1.0 - high) - wear * P.polish * high + markRough, 0.05, 1.0);
  float hub = smoothstep(0.02, 0.1, length(lp.xy));
  float lusterOn = clamp(in.fxI.y, 0.0, 1.0) * smoothstep(P.lusterMinPx * 0.7, P.lusterMinPx, px);
  float lusterSlope = (P.luster * in.surf.w * hub + P.rimLuster * rimTop)
      * (1.0 - wear) * (1.0 - high * wear) * (1.0 - cut) * lusterOn;
  float3 across = normalize(float3(-lp.y, lp.x, 0.0) + float3(1e-5, 0.0, 0.0));
  float occlusion = clamp(1.0 - P.engraveDark * cut, 0.1, 1.0) * reedAO * markShade;
  float grime = cut * wear * P.grime;

  // ---- lighting ----
  float3 nW = normalize(toWorld(in, n));
  float3 du = dfdx(nW);
  float3 dv = dfdy(nW);
  float aaKernel = min(P.aaVariance * (dot(du, du) + dot(dv, dv)) * 2.0, P.aaThreshold);
  float reedBlur = (1.0 - reedOn) * in.surf.z * (reeds > 0.5 ? 0.03 : 0.0);
  float reliefHaze = min(reliefVar, 0.4) * mix(1.0, P.rollReliefHaze, inRoll);
  float a2 = rough * rough * rough * rough + aaKernel + reliefHaze + reedBlur;
  float roughness = sqrt(sqrt(a2));
  float3 v = float3(0.0, 0.0, 1.0);
  float ndv = max(dot(nW, v), 1e-4);
  float3 p = tiltDir(toWorld(in, lp * float3(1.0, 1.0, thick)) * P.studioScale, lightTurn(P));
  float pp = dot(p, p) - P.studioRadius * P.studioRadius;

  float3 prefiltered = float3(0.0);
  if (lusterSlope > 0.004) {
    float3 t = toWorld(in, across);
    float3 tw = normalize(t - nW * dot(nW, t));
    float spacing = lusterSlope * 0.5;
    float alpha = roughness * roughness;
    float tapRough = sqrt(sqrt(alpha * alpha + spacing * spacing * 0.25));
    for (int i = 0; i < 6; i++) {
      float3 nk = normalize(nW + tw * (lusterSlope * LUSTER_TAPS[i]));
      prefiltered += envAt(env, linearMip, P, p, pp, reflect(-v, nk), tapRough) * (1.0 / 6.0);
    }
  } else {
    prefiltered = envAt(env, linearMip, P, p, pp, reflect(-v, nW), roughness);
  }
  float3 metal = prefiltered * envBRDFApprox(baseColor, roughness, ndv) * occlusion;
  float3 grimeColor = float3(0.05, 0.043, 0.034) * mix(0.5, 1.0, nW.y * 0.5 + 0.5);
  float3 color = mix(metal, grimeColor, grime) * P.exposure;
  color = mix(color, float3(P.backdropR, P.backdropG, P.backdropB), clamp(in.fxI.z, 0.0, 1.0) * 0.72);

  if (P.debug == 1) color = baseColor;
  if (P.debug == 2) color = pow(nW * 0.5 + 0.5, float3(2.2));
  if (P.debug == 3) color = float3(roughness);
  if (P.debug == 4) color = float3(lusterSlope * 5.0);
  if (P.debug == 5) color = float3(cut);
  if (P.debug == 6) color = prefiltered;

  float3 encoded = srgbEncode(neutralShoulder(color));
  float noise = (fract(sin(dot(in.worldXY, float2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
  return float4(encoded + noise, 1.0);
}
