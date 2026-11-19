// A photograph of a leather wallet, lit as leather, thread and brass by a light that follows the
// phone. A port of paritytech/wallet-card-native's wallet.metal to a SwiftUI layer effect: the
// image itself is the layer, so the relief comes from its own luminance and the silhouette from
// its own alpha, and nothing but the picture has to ship.
//
// The reference reads a material per pixel from a baked region map. This card has two materials
// that matter, leather and the brass button, and the button is a circle, so it is given as one
// instead of as a texture: the thread of the stitching reads as leather, which its relief carries.
#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

namespace wallet {

struct Material { float metal, rough, relief, sheen, coat; };

// The reference's hand-tuned table (assets/card.json).
constant Material leather = { 0.16, 0.57, 0.45, 0.1, 0.22 };
constant Material brass = { 1.0, 0.34, 0.45, 0.0, 0.0 };

// smoothstep spelled out: envC() calls it with edge0 > edge1, which GLSL and MSL leave undefined.
static float ss(float e0, float e1, float x) {
    float t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

static float lum(SwiftUI::Layer layer, float2 p) {
    half4 c = layer.sample(p);
    return dot(float3(c.rgb), float3(0.3, 0.59, 0.11)) * float(c.a);
}

static float alp(SwiftUI::Layer layer, float2 p) {
    return float(layer.sample(p).a);
}

// A small studio: a graded room, a key strip right of centre, a cool fill on the left.
static float3 envC(float3 d, float r) {
    float3 c = mix(float3(0.16, 0.15, 0.14), float3(1.0, 0.97, 0.92), ss(-0.5, 0.8, d.y));
    float w = 0.12 + r * 0.35;
    c += ss(w, 0.0, abs(d.x - 0.3)) * ss(0.1, 0.5, d.y) * mix(1.8, 0.35, r);
    c += ss(w * 1.4, 0.0, abs(d.x + 0.55)) * ss(-0.2, 0.3, d.y) * mix(0.7, 0.15, r) * float3(0.85, 0.9, 1.0);
    return c;
}

// The slope of the image across ±3 texels and three rows, so creases and folds tilt the normal
// while single-pixel pores average out.
static float2 creases(SwiftUI::Layer layer, float2 p, float2 e) {
    float2 ex = float2(e.x * 3.0, 0.0), ey = float2(0.0, e.y * 3.0);
    float2 sx = float2(e.x, 0.0), sy = float2(0.0, e.y);
    float cx = (lum(layer, p + ex) - lum(layer, p - ex)
                + lum(layer, p + ex + sy) - lum(layer, p - ex + sy)
                + lum(layer, p + ex - sy) - lum(layer, p - ex - sy)) / 3.0;
    float cy = (lum(layer, p + ey) - lum(layer, p - ey)
                + lum(layer, p + ey + sx) - lum(layer, p - ey + sx)
                + lum(layer, p + ey - sx) - lum(layer, p - ey - sx)) / 3.0;
    return float2(cx, cy);
}

} // namespace wallet

/// `size` is the layer in points, `tex` the image in pixels, so `size / tex` is one texel.
/// `tilt` is the light's position, x right and y up, about −1…1.
/// `button` is the brass button: centre as a fraction of the layer, radius as a fraction of its width.
[[ stitchable ]]
half4 walletLeather(float2 position,
                    SwiftUI::Layer layer,
                    float2 size,
                    float2 tex,
                    float2 tilt,
                    float3 button) {
    using namespace wallet;

    half4 c = layer.sample(position);
    if (c.a < 0.01h) return half4(0.0h);
    float3 base = float3(c.rgb) / max(float(c.a), 0.001);

    float2 uv = position / size;
    float2 toButton = (uv - button.xy) * float2(1.0, size.y / size.x);
    Material m = length(toButton) <= button.z ? brass : leather;

    float2 e = size / tex;
    float2 slope = creases(layer, position, e);
    float2 e4 = e * 4.0;
    float ax = alp(layer, position + float2(e4.x, 0.0)) - alp(layer, position - float2(e4.x, 0.0));
    float ay = alp(layer, position + float2(0.0, e4.y)) - alp(layer, position - float2(0.0, e4.y));
    // Layer y runs down the screen; the lighting's y runs up.
    float3 N = normalize(float3(-slope.x * m.relief * 7.0 - ax * 1.4, slope.y * m.relief * 7.0 + ay * 1.4, 1.0));

    // Where the photograph is itself in shadow, under the flap and in the folds, no light reaches,
    // but the steep edge of that shadow reads as a steep slope and used to catch a highlight. The
    // darkest luminance within 7 texels, 8 taps around this pixel, finds the shadow and the rim
    // beside it; there the relief is flattened and the highlights, sheen and coat kept out.
    float darkest = dot(base, float3(0.3, 0.59, 0.11));
    for (int k = 0; k < 8; k++) {
        float a = float(k) * 0.785398;
        darkest = min(darkest, lum(layer, position + float2(cos(a), sin(a)) * e * 7.0));
    }
    float occlusion = ss(0.03, 0.14, darkest);
    N = normalize(float3(N.xy * occlusion, N.z));

    float2 t = tilt;
    float3 V = float3(0.0, 0.0, 1.0);
    // The light never lines up with the view: tipped away it only lowers to 0.3, so the leather's
    // narrow highlight stays on creases and stitches instead of flooding every flat pixel.
    float ly = 0.6 + t.y * (t.y < 0.0 ? 0.3 : 1.5);
    float3 L = normalize(float3(0.35 - t.x * 1.5, ly, 0.85));
    float3 H = normalize(L + V);
    float ndl = max(dot(N, L), 0.0), ndh = max(dot(N, H), 0.0);
    float3 R = reflect(-V, N);
    R.xy += float2(-t.x, t.y) * 1.1;
    R = normalize(R);

    float shin = mix(500.0, 10.0, m.rough);
    float spec = precise::pow(ndh, shin) * (1.0 - m.rough) * (1.0 - m.rough) * 2.2 * occlusion;
    float3 dif = base * (0.78 + 0.34 * ndl);
    float3 env = envC(R, m.rough);
    float3 metalC = base * 0.32 * (0.7 + 0.3 * ndl) + env * base * 1.05 + spec * base * 1.8;
    float3 col = mix(dif + float3(spec) * 0.35 * (1.0 - m.metal), metalC, m.metal * 0.7);
    col += m.sheen * precise::pow(ndh, 5.0) * 0.18 * (base + 0.4) * occlusion;
    col += m.coat * occlusion * (precise::pow(ndh, 320.0) * 1.1
                     + envC(reflect(-V, normalize(float3(N.xy * 0.3, 1.0))) + float3(-t.x, t.y, 0.0), 0.05).r * 0.05);

    return half4(half3(col * float(c.a)), c.a);
}
