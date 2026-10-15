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
uniform float uBasin, uRollGloss, uRollReliefHaze, uEnvSharpen, uHaze, uPolish, uTone, uGrime;
uniform float uEngraveDark, uFrost, uWallRough, uAaVariance, uAaThreshold, uReliefBias;
uniform float uReliefGain;      // how much harder than the atlas the struck relief slopes
uniform float uRoomLift;        // light added to the whole studio: its grey walls painted lighter
uniform vec4 uPanels[6];        // three softboxes of our own, two vec4 each: unit direction + cos(half
                                // angle), gain + pad
uniform int uDebug;
uniform vec4 uMetals[12];       // six metals, two vec4 each: f0 + roughness, tone + pad

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
const float WEAR_TONE = 1.0;     // how far toward the dark cast a fully worn coin goes
const float WEAR_DARKEN = 0.45;  // and how much of its reflectance it loses past that
const float FRESH_HAZE = 0.12;   // roughness every coin carries; the metals start near 0.15
const float WEAR_HAZE = 0.75;    // roughness a fully worn coin gains on top, to fully matte

vec2 markHash(float seed) {
  return fract(sin(vec2(seed * 12.9898, seed * 78.233 + 1.7)) * vec2(43758.5453, 22578.1459));
}

// Pits: circular dishes in the field. The slope runs outward from the centre, which under a key
// light from the upper left gives a dark upper wall and a lit lower lip, the way a strike reads.
void addPits(vec2 local, float count, float seed, inout vec2 slope, inout float shade) {
  int total = min(int(count + 0.5), PIT_MAX);

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
void addStreaks(vec2 local, float amount, float seed, inout vec2 slope, inout float rough) {
  int total = min(int(amount * float(STREAK_MAX) + 0.5), STREAK_MAX);
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
  for (int i = 0; i < 3; ++i) {
    vec4 at = uPanels[i * 2];
    float gain = uPanels[i * 2 + 1].x;
    total += gain * smoothstep(at.w - softness, at.w + softness, dot(dir, at.xyz));
  }
  return total;
}

vec3 envAt(vec3 p, float pp, vec3 Rw, float r) {
  vec3 R = tiltDir(Rw, uLightTurn);
  float b = dot(p, R);
  float t = -b + sqrt(max(b * b - pp, 0.0));
  vec3 dir = normalize(p + R * t);
  float lod = clamp(r * uEnvSharpen, 0.0, 1.0) * uEnvMaxLod;
  return textureLod(uEnv, dir, lod).rgb;
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
  float rr = length(lp.xy);
  float k = PI_F / 2.0 / RELIEF_RADIUS;
  float basinSlope = uBasin * k * sin(rr * 2.0 * k) * (rr < RELIEF_RADIUS ? 1.0 : 0.0);
  slope -= lp.xy / max(rr, 1e-4) * basinSlope;

  // ---- wear marks ----
  vec2 markSlope = vec2(0.0);
  float markShade = 1.0;
  float markRough = 0.0;

  if (front && vSurf.w > 0.5) {
    addPits(local, vMarksI.x, vMarksI.z, markSlope, markShade);
    addStreaks(local, vMarksI.y, vMarksI.z, markSlope, markRough);
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
  float toning = wear * uTone * WEAR_TONE * mix(0.9, 1.0, cut);
  vec3 baseColor = m0.xyz * mix(vec3(1.0), m1.xyz, toning) * (1.0 - WEAR_DARKEN * wear);
  float inRoll = 1.0 - clamp(vFxI.y, 0.0, 1.0);
  // The haze goes on walls and fields alike, and the high relief only rubs slightly shinier: a coin
  // nobody can follow is as matte on its edge as on its face, which is what reads side-on.
  float rough = clamp(m0.w + FRESH_HAZE - inRoll * uRollGloss + vSurf.z * uWallRough + cut * uFrost
                      + wear * WEAR_HAZE * (1.0 - 0.3 * high) - wear * uPolish * high + markRough, 0.05, 1.0);
  float hub = smoothstep(0.02, 0.1, length(lp.xy));
  // Off below the minimum size and fading in above it, so a crowded grid takes the single lookup.
  float lusterOn = clamp(vFxI.y, 0.0, 1.0) * smoothstep(uLusterMinPx, uLusterMinPx * 1.4, px);
  float lusterSlope = (uLuster * vSurf.w * hub + uRimLuster * rimTop)
      * (1.0 - wear) * (1.0 - high * wear) * (1.0 - cut) * lusterOn;
  vec3 across = normalize(vec3(-lp.y, lp.x, 0.0) + vec3(1e-5, 0.0, 0.0));
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

  vec3 prefiltered = vec3(0.0);
  if (lusterSlope > 0.004) {
    vec3 t = toWorld(across);
    vec3 tw = normalize(t - nW * dot(nW, t));
    float spacing = lusterSlope * 0.5;
    float alpha = roughness * roughness;
    float tapRough = sqrt(sqrt(alpha * alpha + spacing * spacing * 0.25));
    for (int i = 0; i < 6; i++) {
      vec3 nk = normalize(nW + tw * (lusterSlope * LUSTER_TAPS[i]));
      prefiltered += envAt(p, pp, reflect(-v, nk), tapRough) * (1.0 / 6.0);
    }
  } else {
    prefiltered = envAt(p, pp, reflect(-v, nW), roughness);
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
