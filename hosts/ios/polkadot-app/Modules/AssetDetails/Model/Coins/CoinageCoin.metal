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
  float coinTurnX, coinTurnY, coinTurnZ;  // every coin leaned by hand, for the debug dial; zero in use
  float grain;           // the field's grain: 0 mint luster (radial flow lines), 1 polished or spun
  float exposure, luster, rimLuster, lusterMinPx, studioRadius, studioScale;
  float rollGloss, rollReliefHaze, envSharpen, haze, polish, tone, grime;
  float engraveDark, frost, wallRough, aaVariance, aaThreshold, reliefBias;
  float spinGrain, spinMinPx, rimPolish, wallSatin;
  float brushCover, brushDepth, brushScale, brushStretch;  // handling brushes a worn face
  float wearDarken;      // reflectance a fully worn coin loses past its tone, so it dulls as well as mattes
  float wearMatte;       // how far a fully worn coin's reflection is pulled to the room's average: no mirror left
  float brushMinPx;      // device px across the coin below which the brushing is skipped: it costs, and cannot be seen
  float reliefGain;      // how much harder than the atlas the struck relief slopes
  float roomLift;        // light added to the whole studio: its grey walls painted lighter
  float lamps[20];       // up to four softboxes of our own, each a direction from the coins, the
                         // cosine of its half angle, and how bright it is
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

// Turns a direction about an axis-angle vector: the studio with the phone.
// Replaces upstream's yawDir, which this generalises: a turn about y is the case where the axis is y.
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

vertex VertexOut coinVertex(VertexIn in [[stage_in]], constant CoinParams &P [[buffer(4)]]) {
  VertexOut o;
  float3 scale = float3(in.iPos.w, in.iPos.w, in.iPos.w * in.iRot.w);
  float3 lean = float3(P.coinTurnX, P.coinTurnY, P.coinTurnZ);
  float3 world = in.iPos.xyz + tiltDir(rotateCoin(in.position * scale, in.iRot.xyz), lean);
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
  o.axisX = tiltDir(rotateCoin(float3(1, 0, 0), in.iRot.xyz), lean);
  o.axisY = tiltDir(rotateCoin(float3(0, 1, 0), in.iRot.xyz), lean);
  o.axisZ = tiltDir(rotateCoin(float3(0, 0, 1), in.iRot.xyz), lean);
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
constant float PIT_RADIUS = 0.115;  // a pit is large on purpose: you can count them
constant float PIT_DEPTH = 2.6;
constant float PIT_FLOOR = 0.42;    // how dark the bottom of a pit goes; 1.0 would not darken at all
constant float STREAK_DEPTH = 1.65;
constant int PIT_MAX = 8;
constant int STREAK_MAX = 12;

// How a coin wears, from the recycler's own ladder: a coin nobody can follow is dull, dark and
// matte, and a coin hidden among hundreds is the only bright one. The reference tones a little and
// hazes a little; here every step of the ladder is meant to be told from the next at sixty points,
// and from the side, where a strip of many coins shows nothing but their walls.
//
// Four things move with wear, all linear in it so no two levels collapse into one: the metal goes
// to its own dark cast, then darker still below the room's average; the surface goes from a light
// haze, which even a clean coin carries so no face is a bare mirror, to fully matte; the mint
// luster dies; grime collects in the cut figure.

static float2 markHash(float seed) {
  return fract(sin(float2(seed * 12.9898, seed * 78.233 + 1.7)) * float2(43758.5453, 22578.1459));
}

// Pits: circular dishes in the field. The slope runs outward from the centre, which under a key
// light from the upper left gives a dark upper wall and a lit lower lip, the way a strike reads.
static void addPits(float2 local, float count, int cap, float seed, thread float2 &slope, thread float &shade) {
  int total = min(int(count + 0.5), cap);

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
static void addStreaks(float2 local, float amount, int cap, float seed, thread float2 &slope, thread float &rough) {
  int total = min(int(amount * float(STREAK_MAX) + 0.5), cap);
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

// Brushing: the fine scratches handling leaves on a coin's face, after the TruMatter design
// document, whose single stretched noise field read here as ruled, parallel lines rim to rim. This
// is three such fields instead, each at its own angle, scale and stretch, bent by a slow warp so no
// stroke runs straight, and broken into patches by a coverage mask, so it reads as scuffed satin.
// It comes up faster than wear itself, so the worst coins are brushed all over and a fresh coin
// shows none. Depth and cover are the document's dials, strength per metal its own.
static float brushHash(float2 p) {
  p = fract(p * float2(123.34, 345.45));
  p += dot(p, p + 34.345);
  return fract(p.x * p.y);
}

static float brushNoise(float2 p) {
  float2 i = floor(p), f = fract(p);
  float2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(brushHash(i), brushHash(i + float2(1, 0)), u.x),
             mix(brushHash(i + float2(0, 1)), brushHash(i + float2(1, 1)), u.x), u.y);
}

static float brushField(float2 p) {
  float v = 0.0, a = 0.5;
  for (int i = 0; i < 3; ++i) { v += a * brushNoise(p); p *= 2.03; a *= 0.5; }
  return v;
}

// One layer of strokes: the slope across a stretched field, turned by `angle`.
static float2 brushLayer(float2 local, float angle, float scale, float stretch) {
  float c = cos(angle), s = sin(angle);
  float2 turned = float2(c * local.x - s * local.y, s * local.x + c * local.y);
  float2 p = float2(turned.x * scale * stretch, turned.y * scale);
  const float step = 0.28;
  float b0 = brushField(p);
  float2 across = float2(brushField(p + float2(step, 0.0)) - b0, brushField(p + float2(0.0, step)) - b0) / step;
  return float2(c * across.x + s * across.y, -s * across.x + c * across.y);
}

// The document works in a frame where the coin's radius is about 0.067 of the stage; `local` here
// has a radius of 0.5, so its strokes are scaled by that ratio to land at the same fineness.
constant float BRUSH_FRAME = 0.134;

static void addBrush(float2 local, float wear, float strength, float seed, constant CoinParams &P,
                     thread float2 &slope) {
  // Steeper than linear: a worn coin is scuffed all over, a lightly used one barely at all.
  float amount = P.brushCover * P.brushDepth * strength * pow(wear, 1.6) * 0.6;

  if (amount <= 0.001) { return; }

  float2 h = markHash(seed + 4.4);
  float base = h.x * PI_F;
  float2 q = local * BRUSH_FRAME;
  // A slow warp bends every stroke, so none runs straight across the face.
  float2 warp = float2(brushField(q * 9.0 + h.y * 7.0), brushField(q * 9.0 + 3.3 - h.y * 5.0)) - 0.5;
  q += warp * 0.012;
  // Patches: strokes cover some of the face and spare the rest, differently per coin.
  float cover = smoothstep(0.35, 0.75, brushField(q * 14.0 + seed * 0.37));

  float scale = 340.0 * max(P.brushScale, 0.05);
  float stretch = max(P.brushStretch, 0.01);
  float2 across = brushLayer(q, base, scale, stretch) * 0.5
      + brushLayer(q, base + 1.9, scale * 0.7, stretch * 2.2) * 0.35
      + brushLayer(q, base + 0.9, scale * 1.4, stretch * 3.5) * 0.25;

  slope -= across * amount * (0.35 + 0.65 * cover);
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

// The softbox added over the studio: a disc of light about its direction, with an edge that softens
// as the blur level rises, which is what prefiltering would have done to a panel in the export.
static float onePanel(float3 dir, float3 at, float edge, float gain, float softness) {
  return gain * smoothstep(edge - softness, edge + softness, dot(dir, at));
}

static float panel(constant CoinParams &P, float3 dir, float lod) {
  float softness = 0.04 + 0.25 * lod / max(P.envMaxLod, 1.0);
  float total = 0.0;
  for (int i = 0; i < 4; ++i) {
    if (P.lamps[i * 5 + 4] <= 0.0) { continue; }
    float3 at = float3(P.lamps[i * 5], P.lamps[i * 5 + 1], P.lamps[i * 5 + 2]);
    total += onePanel(dir, at, P.lamps[i * 5 + 3], P.lamps[i * 5 + 4], softness);
  }
  return total;
}

// What the studio reflects from one direction, looked up with parallax against a sphere of
// `studioRadius` around the coins, and for a worn coin pulled toward what it reflects on average,
// which is the roughest level of the cube.
static float3 envAt(texturecube<float> env, sampler s, constant CoinParams &P, float3 p, float pp, float3 Rw, float r,
                    float matte) {
  float3 R = tiltDir(Rw, lightTurn(P));
  float b = dot(p, R);
  float t = -b + sqrt(max(b * b - pp, 0.0));
  float3 dir = normalize(p + R * t);
  float lod = clamp(r * P.envSharpen, 0.0, 1.0) * P.envMaxLod;
  float3 direct = env.sample(s, dir, level(lod)).rgb;
  // A worn coin no longer mirrors the room: its reflection is pulled toward the room's average, so
  // the light's angle still shades it through the normal and the panels, but nothing is seen in it.
  // The average is a whole second lookup per tap, six of them on a coin with luster, so a fresh
  // coin skips it.
  if (matte <= 0.0) return direct;
  float3 average = env.sample(s, dir, level(P.envMaxLod)).rgb;
  return mix(direct, average, matte);
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
  float2 slope = float2(nr.x * side, nr.y) / max(nr.z, 0.2) * P.reliefGain;
  // ---- wear marks ----
  float2 markSlope = float2(0.0);
  float markShade = 1.0;
  float markRough = 0.0;

  if (front && in.surf.w > 0.5) {
    // A grid coin is drawn at about fifty pixels, where the marks read as a few dots and dashes;
    // only the picked coin is large enough to count them, so only it loops over all of them.
    bool large = px >= 64.0;
    addPits(local, in.marksI.x, large ? PIT_MAX : 4, in.marksI.z, markSlope, markShade);
    addStreaks(local, in.marksI.y, large ? STREAK_MAX : 6, in.marksI.z, markSlope, markRough);
  }
  // Brushing covers the face, figure included, as the document's does; only the walls are spared.
  // It is the dearest thing on the face, some fifty noise taps a pixel, and below the detail size
  // its strokes are under a pixel wide, so a strip or a grid of five hundred never pays for it.
  float brushOn = smoothstep(P.brushMinPx, P.brushMinPx * 1.3, px);
  if (in.surf.z < 0.5 && brushOn > 0.0) {
    float brushStrength = metals[int(in.lookI.y) * 2 + 1].w * brushOn;
    addBrush(local, clamp(in.marksI.y, 0.0, 1.0), brushStrength, in.marksI.z, P, markSlope);
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
  float wear = clamp(in.lookI.x, 0.0, 1.0);
  // Wear tones the metal toward its dark cast and past it: a tone above one keeps darkening, which
  // is how a fully worn coin ends up well below the room's average brightness.
  float toning = wear * P.tone * mix(0.55, 1.0, cut);
  float3 baseColor = m0.xyz * mix(float3(1.0), m1.xyz, toning) * (1.0 - P.wearDarken * wear);
  float inRoll = 1.0 - clamp(in.fxI.y, 0.0, 1.0);
  // A tabletop coin's rim is polished bright; its edge wall is satin, so an edge-on strip shows a
  // soft band rather than a hard white one. The haze goes on walls and fields alike, and the high
  // relief only rubs half as shiny, so wear reads side-on too.
  float polishedRim = P.grain * P.rimPolish * rimTop;
  float rough = clamp(m0.w - inRoll * P.rollGloss + in.surf.z * P.wallRough + cut * P.frost
                      + wear * P.haze * (1.0 - 0.3 * high) - wear * P.polish * high + markRough
                      - polishedRim + P.grain * P.wallSatin * in.surf.z, 0.05, 1.0);
  // Grain: mint luster (radial flow lines, grain 0) or spun brushing (concentric grooves, 1). A
  // polished field has no brushing, so its slope is zero and it takes the single lookup. Mint
  // luster is the six-tap path, off below its minimum size and fading in above it, so a crowded
  // grid takes the single lookup too.
  float hub = smoothstep(mix(0.02, 0.004, P.grain), mix(0.1, 0.022, P.grain), length(lp.xy));
  float lusterOn = max(clamp(in.fxI.y, 0.0, 1.0) * smoothstep(P.lusterMinPx, P.lusterMinPx * 1.4, px),
                       P.grain * smoothstep(P.spinMinPx * 0.7, P.spinMinPx, px));
  float fieldGrain = mix(P.luster, P.spinGrain, P.grain);
  float rimGrain = P.rimLuster * (1.0 - P.grain);
  float wearKeeps = 1.0 - wear * mix(1.0, 0.55, P.grain);
  float lusterSlope = (fieldGrain * in.surf.w * hub + rimGrain * rimTop)
      * wearKeeps * (1.0 - high * wear) * (1.0 - cut) * lusterOn;
  float3 across = normalize(mix(float3(-lp.y, lp.x, 0.0), float3(lp.x, lp.y, 0.0), P.grain) + float3(1e-5, 0.0, 0.0));
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

  float matte = wear * P.wearMatte;
  float3 prefiltered = float3(0.0);
  if (lusterSlope > 0.004) {
    float3 t = toWorld(in, across);
    float3 tw = normalize(t - nW * dot(nW, t));
    float spacing = lusterSlope * 0.5;
    float alpha = roughness * roughness;
    float tapRough = sqrt(sqrt(alpha * alpha + spacing * spacing * 0.25));
    for (int i = 0; i < 6; i++) {
      float3 nk = normalize(nW + tw * (lusterSlope * LUSTER_TAPS[i]));
      prefiltered += envAt(env, linearMip, P, p, pp, reflect(-v, nk), tapRough, matte) * (1.0 / 6.0);
    }
  } else {
    prefiltered = envAt(env, linearMip, P, p, pp, reflect(-v, nW), roughness, matte);
  }
  // The room's lift and our own panels go on once per pixel, about the pixel's own mirror
  // direction, rather than inside the luster loop: they are broad, and six of them were waste.
  {
    float3 mirror = tiltDir(reflect(-v, nW), lightTurn(P));
    float lod = clamp(roughness * P.envSharpen, 0.0, 1.0) * P.envMaxLod;
    prefiltered += P.roomLift + panel(P, mirror, lod);
  }
  float3 metal = prefiltered * envBRDFApprox(baseColor, roughness, ndv) * occlusion;
  float3 grimeColor = float3(0.05, 0.043, 0.034) * mix(0.5, 1.0, nW.y * 0.5 + 0.5);
  float3 color = mix(metal, grimeColor, grime) * P.exposure;
  // A receded coin fades toward whatever is behind the view rather than toward a fixed backdrop:
  // the card it sits on is light in one theme and dark in the other, and only the compositor knows.
  float alpha = 1.0 - clamp(in.fxI.z, 0.0, 1.0) * 0.72;

  if (P.debug == 1) color = baseColor;
  if (P.debug == 2) color = pow(nW * 0.5 + 0.5, float3(2.2));
  if (P.debug == 3) color = float3(roughness);
  if (P.debug == 4) color = float3(lusterSlope * 5.0);
  if (P.debug == 5) color = float3(cut);
  if (P.debug == 6) color = prefiltered;

  float3 encoded = srgbEncode(neutralShoulder(color));
  float noise = (fract(sin(dot(in.worldXY, float2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
  // Premultiplied, which is what the layer composites.
  return float4((encoded + noise) * alpha, alpha);
}
