#version 300 es
// Bakes one face of one level of the studio cube the coin shader samples. GLSL ES 3.0 has no compute
// shaders, so each face of each level is a pass of this over a half-float render target; it is the
// same evaluation as CoinageStudio.kt and as iOS's CoinageStudioBake.metal, and the GGX sample
// directions are drawn here from the Hammersley set rather than uploaded, since the roughest level's
// list would not fit the uniform space ES 3.0 guarantees.
precision highp float;
precision highp int;

uniform int uFace;
uniform float uSize;          // this level's face size
uniform float uAlpha;         // GGX alpha of this level's roughness; unused on level 0
uniform int uSampleCount;     // 0 for the sharp level
uniform int uLampCount;
uniform vec2 uRoom;           // low, high
uniform vec4 uLamps[96];      // six vec4 per lamp: center, uAxis, vAxis, normal, tint, (halfU, halfV, intensity, roomHere)

out vec4 fragColor;

const float PI_F = 3.14159265358979;

vec3 faceDirection(int face, float s, float t) {
  if (face == 0) return vec3(1.0, -t, -s);
  if (face == 1) return vec3(-1.0, -t, s);
  if (face == 2) return vec3(s, 1.0, t);
  if (face == 3) return vec3(s, -1.0, -t);
  if (face == 4) return vec3(s, -t, 1.0);
  return vec3(-s, -t, -1.0);
}

vec3 radiance(vec3 d) {
  float best = 1e30;
  int hit = -1;
  float hu = 0.0;
  float hv = 0.0;

  for (int i = 0; i < uLampCount; ++i) {
    vec3 normal = uLamps[i * 6 + 3].xyz;
    float denom = dot(d, normal);
    if (denom >= -1e-6) { continue; }
    vec3 center = uLamps[i * 6].xyz;
    float t = dot(center, normal) / denom;
    if (t <= 0.0 || t >= best) { continue; }
    vec3 p = d * t - center;
    vec4 extent = uLamps[i * 6 + 5];
    float lu = dot(p, uLamps[i * 6 + 1].xyz) / extent.x;
    float lv = dot(p, uLamps[i * 6 + 2].xyz) / extent.y;
    if (abs(lu) > 1.0 || abs(lv) > 1.0) { continue; }
    best = t;
    hit = i;
    hu = lu;
    hv = lv;
  }

  if (hit >= 0) {
    vec4 extent = uLamps[hit * 6 + 5];
    float f = extent.z * (1.0 - 0.12 * (hu * hu + hv * hv));
    return uLamps[hit * 6 + 4].xyz * f + extent.w;
  }

  float g = uRoom.x + (uRoom.y - uRoom.x) * smoothstep(-0.35, 0.85, d.y);
  return vec3(g);
}

float radicalInverse(uint bits) {
  bits = (bits << 16u) | (bits >> 16u);
  bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
  bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
  bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
  bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
  return float(bits) * 2.3283064365386963e-10;
}

void main() {
  // Framebuffer row 0 is the texture's row 0, which is the convention's row 0.
  vec2 texel = gl_FragCoord.xy - 0.5;

  if (uSampleCount == 0) {
    vec3 sum = vec3(0.0);
    for (int sy = 0; sy < 2; ++sy) {
      for (int sx = 0; sx < 2; ++sx) {
        float s = 2.0 * (texel.x + (float(sx) + 0.5) / 2.0) / uSize - 1.0;
        float t = 2.0 * (texel.y + (float(sy) + 0.5) / 2.0) / uSize - 1.0;
        sum += radiance(normalize(faceDirection(uFace, s, t)));
      }
    }
    fragColor = vec4(sum * 0.25, 1.0);
    return;
  }

  float s = 2.0 * (texel.x + 0.5) / uSize - 1.0;
  float t = 2.0 * (texel.y + 0.5) / uSize - 1.0;
  vec3 N = normalize(faceDirection(uFace, s, t));
  vec3 up = abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
  vec3 T = normalize(cross(up, N));
  vec3 B = cross(N, T);
  float alpha2 = uAlpha * uAlpha;

  vec3 sum = vec3(0.0);
  float weight = 0.0;
  for (int k = 0; k < uSampleCount; ++k) {
    float e1 = (float(k) + 0.5) / float(uSampleCount);
    float e2 = radicalInverse(uint(k));
    float phi = 2.0 * PI_F * e1;
    float cosTheta = sqrt((1.0 - e2) / (1.0 + (alpha2 - 1.0) * e2));
    float sinTheta = sqrt(1.0 - cosTheta * cosTheta);
    vec3 h = vec3(sinTheta * cos(phi), sinTheta * sin(phi), cosTheta);
    vec3 H = T * h.x + B * h.y + N * h.z;
    vec3 L = 2.0 * h.z * H - N;
    float nl = dot(L, N);
    if (nl <= 0.0) { continue; }
    sum += radiance(L) * nl;
    weight += nl;
  }
  fragColor = vec4(sum / max(weight, 1e-6), 1.0);
}
