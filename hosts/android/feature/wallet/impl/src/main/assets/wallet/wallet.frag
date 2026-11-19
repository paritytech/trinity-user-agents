#version 300 es
// The "Material" view of Material Detector.dc.html (FRAG), ported to GLSL ES 3.00.
// The math is unchanged; removed: region highlight (uHi), mask view (uMask), pulse (uT).
// smoothstep() is spelled out as ss() because envC() calls it with edge0 > edge1, which is
// undefined in GLSL and MSL; ss() is the formula WebGL drivers evaluate.
precision highp float;
in vec2 vUv;
out vec4 fragColor;

uniform sampler2D uImg;   // image.png, premultiplied RGBA, LINEAR, no mipmaps
uniform sampler2D uReg;   // regions.png, R8, NEAREST
uniform vec2 uTex;        // image.png size in pixels
uniform vec2 uTilt;       // smoothed tilt, x right, y up, each -1..1
uniform float uMetal[6], uRough[6], uRelief[6], uSheen[6], uCoat[6];
uniform float uCrease;    // params.json relief.creases: 0 = the detector's 1-texel relief, 1 = crease scale
uniform float uGuard;     // params.json light.guard: 0 = the detector's light path, 1 = never head-on
uniform float uOcclusion;    // params.json light.occlusion: 1 = the photo's shadows take no highlight, 0 = the detector

float ss(float e0, float e1, float x) { float t = clamp((x - e0) / (e1 - e0), 0., 1.); return t * t * (3. - 2. * t); }
float lum(vec2 p) { vec4 c = texture(uImg, p); return dot(c.rgb, vec3(.3, .59, .11)) * c.a; }
float alp(vec2 p) { return texture(uImg, p).a; }

// The studio: a graded room, a key strip right of centre and a cooler fill on the left.
vec3 envC(vec3 d, float r) {
  vec3 c = mix(vec3(.16, .15, .14), vec3(1., .97, .92), ss(-.5, .8, d.y));
  float w = .12 + r * .35;
  c += ss(w, 0., abs(d.x - .3)) * ss(.1, .5, d.y) * mix(1.8, .35, r);
  c += ss(w * 1.4, 0., abs(d.x + .55)) * ss(-.2, .3, d.y) * mix(.7, .15, r) * vec3(.85, .9, 1.);
  return c;
}

void main() {
  vec2 uv = vUv;
  vec4 c = texture(uImg, uv);
  if (c.a < .01) { fragColor = vec4(0.); return; }
  vec3 base = c.rgb / max(c.a, .001);

  // region → material; pixels outside every region keep the detector's defaults
  float id = floor(texture(uReg, uv).r * 255. / 40. + .5) - 1.;
  float me = 0., ro = .6, re = .5, sh = 0., co = 0.;
  int i = int(id);
  if (i >= 0 && i < 6) { me = uMetal[i]; ro = uRough[i]; re = uRelief[i]; sh = uSheen[i]; co = uCoat[i]; }

  // normal from luminance (relief) and from alpha (the silhouette rolls off)
  vec2 e = 1. / uTex;
  float hx = lum(uv + vec2(e.x, 0.)) - lum(uv - vec2(e.x, 0.));
  float hy = lum(uv + vec2(0., e.y)) - lum(uv - vec2(0., e.y));
  // not in the detector: the same slope measured across ±3 texels and three rows, so creases and
  // folds tilt the normal while single-pixel pores average out (they made sharp glints)
  if (uCrease > 0.) {
    vec2 ex = vec2(e.x * 3., 0.), ey = vec2(0., e.y * 3.), sx = vec2(e.x, 0.), sy = vec2(0., e.y);
    float cx = (lum(uv + ex) - lum(uv - ex) + lum(uv + ex + sy) - lum(uv - ex + sy) + lum(uv + ex - sy) - lum(uv - ex - sy)) / 3.;
    float cy = (lum(uv + ey) - lum(uv - ey) + lum(uv + ey + sx) - lum(uv - ey + sx) + lum(uv + ey - sx) - lum(uv - ey - sx)) / 3.;
    hx = mix(hx, cx, uCrease);
    hy = mix(hy, cy, uCrease);
  }
  vec2 e3 = e * 4.;
  float ax = alp(uv + vec2(e3.x, 0.)) - alp(uv - vec2(e3.x, 0.));
  float ay = alp(uv + vec2(0., e3.y)) - alp(uv - vec2(0., e3.y));
  vec3 N = normalize(vec3(-hx * re * 7. - ax * 1.4, hy * re * 7. + ay * 1.4, 1.));

  // not in the detector: where the photo itself is in shadow (under the flap, in folds) the light can't reach,
  // but the steep edge of that shadow reads as a steep slope. Take the darkest photo luminance within 7 texels
  // (8 taps + this pixel), so the rim beside a shadow counts too, and there flatten the relief and keep
  // highlights, sheen and coat out.
  float lmin = dot(base, vec3(.3, .59, .11));
  for (int k = 0; k < 8; k++) {
    float a = float(k) * 0.785398;
    lmin = min(lmin, lum(uv + vec2(cos(a), sin(a)) * e * 7.));
  }
  float occ = mix(1., ss(.03, .14, lmin), uOcclusion);
  N = normalize(vec3(N.xy * mix(1., occ, uOcclusion), N.z));

  // tilt moves the light and the reflected studio, never the geometry
  vec3 V = vec3(0., 0., 1.);
  // not in the detector: with uGuard, tipping the top away lowers the light only to .3 (≈ 19° off the view axis),
  // so the narrow highlight can never line up with the flat leather and flood the whole card
  float ly = .6 + uTilt.y * (uTilt.y < 0. ? mix(1.5, .3, uGuard) : 1.5);
  vec3 L = normalize(vec3(.35 - uTilt.x * 1.5, ly, .85));
  vec3 H = normalize(L + V);
  float ndl = max(dot(N, L), 0.), ndh = max(dot(N, H), 0.);
  vec3 R = reflect(-V, N); R.xy += vec2(-uTilt.x, uTilt.y) * 1.1; R = normalize(R);

  float shin = mix(500., 10., ro);
  float spec = pow(ndh, shin) * (1. - ro) * (1. - ro) * 2.2 * occ;
  vec3 dif = base * (.78 + .34 * ndl);
  vec3 env = envC(R, ro);
  vec3 metalC = base * .32 * (.7 + .3 * ndl) + env * base * 1.05 + spec * base * 1.8;
  vec3 col = mix(dif + vec3(spec) * .35 * (1. - me), metalC, me * .7);
  col += sh * pow(ndh, 5.) * .18 * (base + .4) * occ;                             // sheen (thread, fabric)
  col += co * occ * (pow(ndh, 320.) * 1.1                                         // clear coat
             + envC(reflect(-V, normalize(vec3(N.xy * .3, 1.))) + vec3(-uTilt.x, uTilt.y, 0.), .05).r * .05);

  fragColor = vec4(col * c.a, c.a);
}
