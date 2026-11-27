#version 300 es
// Coin fragment shader, GLSL ES 3.0. Line-by-line port of CoinageCoin.metal's `coinFragment`, which
// is itself a port of the reference's native/shaders/coin.frag; section names match.
//
// Differences from the Metal version, all mechanical: `sample(..., level(lod))` becomes `textureLod`,
// `thread T&` becomes `inout T`, and the parameter block arrives as loose uniforms rather than as one
// struct — there is no host struct for its size and alignment to disagree with.
precision highp float;
precision highp int;
precision highp sampler2D;
precision highp samplerCube;

in vec3 vPos;
in vec3 vNormal;
in vec4 vSurf;
in vec2 vWorldXY;
flat in vec4 vPosI;
flat in vec4 vRotI;
flat in vec4 vLookI;
flat in vec4 vFxI;
flat in vec4 vMarksI;
flat in vec3 vAxisX;
flat in vec3 vAxisY;
flat in vec3 vAxisZ;

uniform float uDpr;             // device pixels per point
uniform float uTilePx;          // atlas tile size
uniform float uEnvMaxLod;       // cube levels - 1
uniform vec3 uLightTurn;        // studio turned with the phone: an axis scaled by the angle turned
uniform float uExposure, uLuster, uRimLuster, uLusterMinPx, uStudioRadius, uStudioScale;
uniform float uRollGloss, uRollReliefHaze, uEnvSharpen, uHaze, uPolish, uTone, uGrime;
uniform float uEngraveDark, uFrost, uWallRough, uAaVariance, uAaThreshold, uReliefBias;
uniform float uGrain;           // the field's grain: 0 mint luster (radial flow lines), 1 polished or spun
uniform float uSpinGrain, uSpinMinPx, uRimPolish, uWallSatin;
uniform float uBrushCover, uBrushDepth, uBrushScale, uBrushStretch;  // handling brushes a worn face
uniform float uWearDarken;      // reflectance a fully worn coin loses past its tone, so it dulls as well as mattes
uniform float uWearMatte;       // how far a fully worn coin's reflection is pulled to the room's average
uniform float uBrushMinPx;      // device px across the coin below which the brushing is skipped
uniform float uReliefGain;      // how much harder than the atlas the struck relief slopes
uniform float uRoomLift;        // light added to the whole studio: its grey walls painted lighter
uniform vec4 uPanels[8];        // up to four softboxes of our own, two vec4 each: unit direction +
                                // cos(half angle), gain + pad; a slot with no gain is skipped
uniform int uDebug;
uniform vec4 uMetals[14];       // seven metals, two vec4 each: f0 + roughness, tone + brushing strength

uniform sampler2D uAtlas;
uniform samplerCube uEnv;

out vec4 fragColor;

const float ATLAS_TILES = 4.0;
const float REVERSE_TILE = 15.0;
const float RELIEF_RADIUS = 0.37;
const float PI_F = 3.14159265358979;
const float LUSTER_TAPS[6] = float[6](-1.25, -0.75, -0.25, 0.25, 0.75, 1.25);

vec3 toWorld(vec3 v) { return vAxisX * v.x + vAxisY * v.y + vAxisZ * v.z; }

// Wear marks. Two things happen to a coin and they are deliberately different to look at.
//
// Every payment a coin has been through leaves a pit: a struck dish, few and large, that reads as
// damage done TO the coin. How far its recycler still has to go leaves streaks: many fine scratches
// that read as a dull, scuffed surface. A coin nobody can follow is clean of both.
//
// Both are procedural and seeded per coin, so a coin keeps its own face across frames without any
// storage. Both are pushed harder than the reference's own wear, which is legible on a large render
// and all but invisible at strip size.
const float MARK_INNER = 0.30;   // clear of the struck figure, which has to stay readable
const float MARK_OUTER = 0.45;   // out to the rim
const float PIT_RADIUS = 0.115;  // a pit is large on purpose: you can count them
const float PIT_DEPTH = 2.6;
const float PIT_FLOOR = 0.42;    // how dark the bottom of a pit goes; 1.0 would not darken at all
const float STREAK_DEPTH = 1.65;
const int PIT_MAX = 8;
const int STREAK_MAX = 12;

// How a coin wears, from the recycler's own ladder: a coin nobody can follow is dull, dark and
// matte, and a coin hidden among hundreds is the only bright one. Every step of the ladder is meant
// to be told from the next at sixty points, and from the side, where a strip shows only walls.
//
// All linear in wear, so no two levels collapse into one: the metal goes to its own dark cast, then
// darker still below the room's average; the surface goes from a light haze, which even a clean
// coin carries so no face is a bare mirror, to fully matte; the mint luster dies; grime collects
// in the cut figure.

vec2 markHash(float seed) {
  return fract(sin(vec2(seed * 12.9898, seed * 78.233 + 1.7)) * vec2(43758.5453, 22578.1459));
}

// Pits: circular dishes in the field. The slope runs outward from the centre, which under a key
// light from the upper left gives a dark upper wall and a lit lower lip, the way a strike reads.
void addPits(vec2 local, float count, int cap, float seed, inout vec2 slope, inout float shade) {
  int total = min(int(count + 0.5), cap);

  for (int i = 0; i < total; ++i) {
    vec2 h = markHash(seed + float(i) * 7.13);
    float angle = h.x * 2.0 * PI_F;
    float ring = mix(MARK_INNER, MARK_OUTER, h.y);
    vec2 delta = local - vec2(cos(angle), sin(angle)) * ring;
    float dist = length(delta) / PIT_RADIUS;

    if (dist >= 1.0) { continue; }

    float bowl = 1.0 - dist * dist;
    slope += normalize(delta + vec2(1e-5, 0.0)) * PIT_DEPTH * dist * bowl;
    shade *= mix(1.0, PIT_FLOOR, bowl * bowl);
  }
}

// Streaks: short grooves at random angles, as many as the coin is still traceable. They fade out
// over the struck figure, so a heavily scuffed coin still reads its own value.
void addStreaks(vec2 local, float amount, int cap, float seed, inout vec2 slope, inout float rough) {
  int total = min(int(amount * float(STREAK_MAX) + 0.5), cap);
  float clear = smoothstep(MARK_INNER * 0.72, MARK_INNER, length(local));

  if (clear <= 0.0) { return; }

  for (int i = 0; i < total; ++i) {
    vec2 h = markHash(seed + 31.7 + float(i) * 3.77);
    vec2 g = markHash(seed + 91.3 + float(i) * 5.21);
    float angle = h.x * PI_F;
    vec2 along = vec2(cos(angle), sin(angle));
    vec2 across = vec2(-along.y, along.x);
    vec2 centre = (g - 0.5) * 2.0 * MARK_OUTER;
    vec2 delta = local - centre;
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
vec3 tiltDir(vec3 d, vec3 turn) {
  float angle = length(turn);
  if (angle < 1e-6) return d;
  vec3 axis = turn / angle;
  float c = cos(angle), s = sin(angle);
  return d * c + cross(axis, d) * s + axis * dot(axis, d) * (1.0 - c);
}

// Brushing: the fine scratches handling leaves on a coin's face. Three stretched noise fields at their
// own angles, scales and stretches, bent by a slow warp and broken into patches, so it reads as scuffed
// satin rather than ruled lines. It comes up faster than wear itself.
float brushHash(vec2 p) {
  p = fract(p * vec2(123.34, 345.45));
  p += dot(p, p + 34.345);
  return fract(p.x * p.y);
}

float brushNoise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(brushHash(i), brushHash(i + vec2(1.0, 0.0)), u.x),
             mix(brushHash(i + vec2(0.0, 1.0)), brushHash(i + vec2(1.0, 1.0)), u.x), u.y);
}

float brushField(vec2 p) {
  float v = 0.0, a = 0.5;
  for (int i = 0; i < 3; ++i) { v += a * brushNoise(p); p *= 2.03; a *= 0.5; }
  return v;
}

vec2 brushLayer(vec2 local, float angle, float scale, float stretch) {
  float c = cos(angle), s = sin(angle);
  vec2 turned = vec2(c * local.x - s * local.y, s * local.x + c * local.y);
  vec2 p = vec2(turned.x * scale * stretch, turned.y * scale);
  const float stepSize = 0.28;
  float b0 = brushField(p);
  vec2 across = vec2(brushField(p + vec2(stepSize, 0.0)) - b0, brushField(p + vec2(0.0, stepSize)) - b0) / stepSize;
  return vec2(c * across.x + s * across.y, -s * across.x + c * across.y);
}

// The design document's frame has a coin radius of about 0.067; `local` has 0.5.
const float BRUSH_FRAME = 0.134;

void addBrush(vec2 local, float wear, float strength, float seed, inout vec2 slope) {
  float amount = uBrushCover * uBrushDepth * strength * pow(wear, 1.6) * 0.6;

  if (amount <= 0.001) { return; }

  vec2 h = markHash(seed + 4.4);
  float base = h.x * PI_F;
  vec2 q = local * BRUSH_FRAME;
  vec2 warp = vec2(brushField(q * 9.0 + h.y * 7.0), brushField(q * 9.0 + 3.3 - h.y * 5.0)) - 0.5;
  q += warp * 0.012;
  float cover = smoothstep(0.35, 0.75, brushField(q * 14.0 + seed * 0.37));

  float scale = 340.0 * max(uBrushScale, 0.05);
  float stretch = max(uBrushStretch, 0.01);
  vec2 across = brushLayer(q, base, scale, stretch) * 0.5
      + brushLayer(q, base + 1.9, scale * 0.7, stretch * 2.2) * 0.35
      + brushLayer(q, base + 0.9, scale * 1.4, stretch * 3.5) * 0.25;

  slope -= across * amount * (0.35 + 0.65 * cover);
}

vec3 envBRDFApprox(vec3 f0, float rough, float ndv) {
  vec4 r = vec4(-1.0, -0.0275, -0.572, 0.022) * rough + vec4(1.0, 0.0425, 1.04, -0.04);
  float a004 = min(r.x * r.x, exp2(-9.28 * ndv)) * r.x + r.y;
  vec2 ab = vec2(-1.04, 1.04) * a004 + r.zw;
  return f0 * ab.x + ab.y;
}

vec3 neutralShoulder(vec3 c) {
  const float start = 0.76;
  const float d = 1.0 - start;
  float peak = max(max(c.r, c.g), max(c.b, 1e-6));
  if (peak <= start) return c;
  float newPeak = 1.0 - d * d / (peak + d - start);
  c *= newPeak / peak;
  float g = 1.0 - 1.0 / ((peak - newPeak) * 0.15 + 1.0);
  return mix(c, vec3(newPeak), g);
}

vec3 srgbEncode(vec3 c) {
  vec3 lo = c * 12.92;
  vec3 hi = pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) * 1.055 - 0.055;
  return mix(hi, lo, vec3(lessThanEqual(c, vec3(0.0031308))));
}

// A softbox added over the studio: a disc of light about its direction, with an edge that softens
// as the blur level rises, which is what prefiltering would have done to a panel in the export.
float panels(vec3 dir, float lod) {
  float softness = 0.04 + 0.25 * lod / max(uEnvMaxLod, 1.0);
  float total = 0.0;
  for (int i = 0; i < 4; ++i) {
    vec4 at = uPanels[i * 2];
    float gain = uPanels[i * 2 + 1].x;
    if (gain <= 0.0) { continue; }
    total += gain * smoothstep(at.w - softness, at.w + softness, dot(dir, at.xyz));
  }
  return total;
}

// A worn coin's reflection is pulled toward the room's average, the roughest level, so the light still
// shades it through the normal and the panels but nothing is mirrored in it. A fresh coin skips the
// second lookup.
vec3 envAt(vec3 p, float pp, vec3 Rw, float r, float matte) {
  vec3 R = tiltDir(Rw, uLightTurn);
  float b = dot(p, R);
  float t = -b + sqrt(max(b * b - pp, 0.0));
  vec3 dir = normalize(p + R * t);
  float lod = clamp(r * uEnvSharpen, 0.0, 1.0) * uEnvMaxLod;
  vec3 direct = textureLod(uEnv, dir, lod).rgb;
  if (matte <= 0.0) return direct;
  return mix(direct, textureLod(uEnv, dir, uEnvMaxLod).rgb, matte);
}

void main() {
  vec3 lp = vPos;
  float thick = vRotI.w;
  vec3 n = normalize(vNormal / vec3(1.0, 1.0, max(thick, 1e-3)));
  float px = vPosI.w * uDpr;

  // ---- relief ----
  bool front = lp.z > 0.0;
  float side = front ? 1.0 : -1.0;
  float tile = front ? vLookI.w : REVERSE_TILE;
  vec2 tileXY = vec2(mod(tile, ATLAS_TILES), floor(tile / ATLAS_TILES));
  vec2 local = vec2(lp.x * side, lp.y);
  vec2 uv = (tileXY + vec2(local.x + 0.5, 0.5 - local.y)) / ATLAS_TILES;
  float lod = clamp(log2(uTilePx / max(px, 1.0)) + uReliefBias, 0.0, 7.0);
  vec4 texel = textureLod(uAtlas, uv, lod);
  vec3 nr = texel.rgb * 2.0 - 1.0;
  float nrLen = max(length(nr), 1e-3);
  float height = texel.a * 2.0 - 1.0;
  float mask = vSurf.w * (1.0 - smoothstep(RELIEF_RADIUS - 0.015, RELIEF_RADIUS + 0.01, length(lp.xy)));
  vec2 slope = vec2(nr.x * side, nr.y) / max(nr.z, 0.2) * uReliefGain;

  // ---- wear marks ----
  vec2 markSlope = vec2(0.0);
  float markShade = 1.0;
  float markRough = 0.0;

  if (front && vSurf.w > 0.5) {
    // Only a coin large enough to count its marks loops over all of them.
    bool large = px >= 64.0;
    addPits(local, vMarksI.x, large ? PIT_MAX : 4, vMarksI.z, markSlope, markShade);
    addStreaks(local, vMarksI.y, large ? STREAK_MAX : 6, vMarksI.z, markSlope, markRough);
  }
  // Brushing covers the face, figure included, and spares the walls. Some fifty noise taps a pixel, and
  // under a pixel wide below its minimum size, so a strip or a large grid never pays for it.
  float brushOn = smoothstep(uBrushMinPx, uBrushMinPx * 1.3, px);
  if (vSurf.z < 0.5 && brushOn > 0.0) {
    float brushStrength = uMetals[int(vLookI.y) * 2 + 1].w * brushOn;
    addBrush(local, clamp(vMarksI.y, 0.0, 1.0), brushStrength, vMarksI.z, markSlope);
  }

  slope += markSlope;
  n = normalize(n + vec3(slope, 0.0) * mask);
  float reliefVar = max((1.0 - nrLen) / nrLen, 0.0) * mask;
  float cut = clamp((-height - 0.05) * 1.3, 0.0, 1.0) * mask;
  float rimTop = (1.0 - vSurf.w) * (1.0 - vSurf.z);
  float high = max(clamp(height * 1.6, 0.0, 1.0) * mask, rimTop * 0.8);

  // ---- edge calm ----
  float calm = clamp(vFxI.w, 0.0, 1.0) * vSurf.z;
  vec3 radial = normalize(vec3(lp.x, lp.y, 0.0) + vec3(1e-5, 0.0, 0.0));
  n = normalize(mix(n, vec3(radial.x, radial.y, n.z), calm));

  // ---- reeded edge ----
  float reeds = vFxI.x * (1.0 - calm);
  float phase = vSurf.y * reeds;
  float pxPerReed = px * PI_F / max(reeds, 1.0);
  float reedOn = smoothstep(2.2, 4.5, pxPerReed) * vSurf.z * (reeds > 0.5 ? 1.0 : 0.0);
  vec3 tangent = vec3(-n.y, n.x, 0.0);
  n = normalize(n + tangent * (sin(phase * 2.0 * PI_F) * 0.6 * reedOn));
  float reedAO = mix(1.0, 0.62, (cos(phase * 2.0 * PI_F) * 0.5 + 0.5) * reedOn);

  // ---- standard inputs ----
  bool isCore = vSurf.x > 0.5 && vLookI.z > -0.5;
  int metalIndex = int(isCore ? vLookI.z : vLookI.y);
  vec4 m0 = uMetals[metalIndex * 2];
  vec4 m1 = uMetals[metalIndex * 2 + 1];
  float wear = clamp(vLookI.x, 0.0, 1.0);
  // A tone above one keeps darkening past the dark cast, so a fully worn coin ends up below the room's average.
  float toning = wear * uTone * mix(0.55, 1.0, cut);
  vec3 baseColor = m0.xyz * mix(vec3(1.0), m1.xyz, toning) * (1.0 - uWearDarken * wear);
  float inRoll = 1.0 - clamp(vFxI.y, 0.0, 1.0);
  // A polished rim and a satin wall for a plain coin; the haze goes on walls and fields alike, so wear
  // reads side-on too.
  float polishedRim = uGrain * uRimPolish * rimTop;
  float rough = clamp(m0.w - inRoll * uRollGloss + vSurf.z * uWallRough + cut * uFrost
                      + wear * uHaze * (1.0 - 0.3 * high) - wear * uPolish * high + markRough
                      - polishedRim + uGrain * uWallSatin * vSurf.z, 0.05, 1.0);
  // Grain: mint luster (radial, grain 0) or spun (concentric, 1). The six-tap path is off below its
  // minimum size, so a crowded grid takes the single lookup.
  float hub = smoothstep(mix(0.02, 0.004, uGrain), mix(0.1, 0.022, uGrain), length(lp.xy));
  float lusterOn = max(clamp(vFxI.y, 0.0, 1.0) * smoothstep(uLusterMinPx, uLusterMinPx * 1.4, px),
                       uGrain * smoothstep(uSpinMinPx * 0.7, uSpinMinPx, px));
  float fieldGrain = mix(uLuster, uSpinGrain, uGrain);
  float rimGrain = uRimLuster * (1.0 - uGrain);
  float wearKeeps = 1.0 - wear * mix(1.0, 0.55, uGrain);
  float lusterSlope = (fieldGrain * vSurf.w * hub + rimGrain * rimTop)
      * wearKeeps * (1.0 - high * wear) * (1.0 - cut) * lusterOn;
  vec3 across = normalize(mix(vec3(-lp.y, lp.x, 0.0), vec3(lp.x, lp.y, 0.0), uGrain) + vec3(1e-5, 0.0, 0.0));
  float occlusion = clamp(1.0 - uEngraveDark * cut, 0.1, 1.0) * reedAO * markShade;
  float grime = cut * wear * uGrime;

  // ---- lighting ----
  vec3 nW = normalize(toWorld(n));
  vec3 du = dFdx(nW);
  vec3 dv = dFdy(nW);
  float aaKernel = min(uAaVariance * (dot(du, du) + dot(dv, dv)) * 2.0, uAaThreshold);
  float reedBlur = (1.0 - reedOn) * vSurf.z * (reeds > 0.5 ? 0.03 : 0.0);
  float reliefHaze = min(reliefVar, 0.4) * mix(1.0, uRollReliefHaze, inRoll);
  float a2 = rough * rough * rough * rough + aaKernel + reliefHaze + reedBlur;
  float roughness = sqrt(sqrt(a2));
  vec3 v = vec3(0.0, 0.0, 1.0);
  float ndv = max(dot(nW, v), 1e-4);
  vec3 p = tiltDir(toWorld(lp * vec3(1.0, 1.0, thick)) * uStudioScale, uLightTurn);
  float pp = dot(p, p) - uStudioRadius * uStudioRadius;

  float matte = wear * uWearMatte;
  vec3 prefiltered = vec3(0.0);
  if (lusterSlope > 0.004) {
    vec3 t = toWorld(across);
    vec3 tw = normalize(t - nW * dot(nW, t));
    float spacing = lusterSlope * 0.5;
    float alpha = roughness * roughness;
    float tapRough = sqrt(sqrt(alpha * alpha + spacing * spacing * 0.25));
    for (int i = 0; i < 6; i++) {
      vec3 nk = normalize(nW + tw * (lusterSlope * LUSTER_TAPS[i]));
      prefiltered += envAt(p, pp, reflect(-v, nk), tapRough, matte) * (1.0 / 6.0);
    }
  } else {
    prefiltered = envAt(p, pp, reflect(-v, nW), roughness, matte);
  }
  // Broad, so once per pixel about its own mirror direction rather than once per luster tap.
  {
    vec3 mirror = tiltDir(reflect(-v, nW), uLightTurn);
    float envLod = clamp(roughness * uEnvSharpen, 0.0, 1.0) * uEnvMaxLod;
    prefiltered += uRoomLift + panels(mirror, envLod);
  }
  vec3 metal = prefiltered * envBRDFApprox(baseColor, roughness, ndv) * occlusion;
  vec3 grimeColor = vec3(0.05, 0.043, 0.034) * mix(0.5, 1.0, nW.y * 0.5 + 0.5);
  vec3 color = mix(metal, grimeColor, grime) * uExposure;
  // A receded coin fades toward whatever is behind the surface rather than toward a fixed backdrop:
  // the card is light in one theme and dark in the other.
  float alpha = 1.0 - clamp(vFxI.z, 0.0, 1.0) * 0.72;

  if (uDebug == 1) color = baseColor;
  if (uDebug == 2) color = pow(nW * 0.5 + 0.5, vec3(2.2));
  if (uDebug == 3) color = vec3(roughness);
  if (uDebug == 4) color = vec3(lusterSlope * 5.0);
  if (uDebug == 5) color = vec3(cut);
  if (uDebug == 6) color = prefiltered;

  vec3 encoded = srgbEncode(neutralShoulder(color));
  float noise = (fract(sin(dot(vWorldXY, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0;
  fragColor = vec4((encoded + noise) * alpha, alpha);
}
