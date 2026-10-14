//
//  HolographicGradient.metal
//  HolographicCard
//

#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

static half3 rampColor(float t) {
    const int N = 20;
    const float pos[20] = {
        0.00, 0.06, 0.11, 0.16, 0.21, 0.26, 0.31, 0.41, 0.46, 0.51,
        0.54, 0.59, 0.63, 0.66, 0.71, 0.76, 0.81, 0.86, 0.91, 1.00
    };
    const half3 col[20] = {
        half3(141, 142, 142) / 255.0h,
        half3(165, 165, 166) / 255.0h,
        half3(178, 179, 179) / 255.0h,
        half3(191, 191, 192) / 255.0h,
        half3(203, 203, 205) / 255.0h,
        half3(214, 215, 217) / 255.0h,
        half3(219, 220, 222) / 255.0h,
        half3(220, 222, 224) / 255.0h, // silver plateau
        half3(210, 212, 224) / 255.0h,
        half3(197, 205, 229) / 255.0h,
        half3(197, 211, 237) / 255.0h, // bluest point (muted, as rendered)
        half3(212, 227, 240) / 255.0h,
        half3(223, 231, 238) / 255.0h,
        half3(238, 238, 240) / 255.0h,
        half3(247, 245, 244) / 255.0h, // white peak
        half3(250, 237, 226) / 255.0h,
        half3(246, 220, 197) / 255.0h, // peach
        half3(223, 199, 178) / 255.0h, // tan
        half3(186, 175, 170) / 255.0h,
        half3(174, 166, 165) / 255.0h  // warm gray
    };

    if (t <= pos[0])     { return col[0]; }
    if (t >= pos[N - 1]) { return col[N - 1]; }
    for (int i = 0; i < N - 1; i++) {
        if (t <= pos[i + 1]) {
            float f = (t - pos[i]) / (pos[i + 1] - pos[i]);
            return mix(col[i], col[i + 1], half(f));
        }
    }
    return col[N - 1];
}

static float hash21(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

[[ stitchable ]]
half4 holographicGradient(float2 position,
                          half4 color, // required by colorEffect, not used
                          float2 size,
                          float2 tilt) {
    float2 centre = size * 0.5;

    // Main gradient axis: 151.367deg (screen space, y-down).
    const float2 dir  = float2(0.4787, 0.8780);
    // Perpendicular axis (points down-left); s=0 is the top-right side.
    const float2 perp = float2(-0.8780, 0.4787);

    // Position along the main axis, normalised 0...1 across the box (CSS-style).
    float halfMain = 0.5 * (abs(dir.x) * size.x + abs(dir.y) * size.y);
    float t = 0.5 + dot(position - centre, dir) / (2.0 * halfMain);
    // Tilt slides the colour band along the axis.
    t += tilt.x * 0.16 - tilt.y * 0.10;
    t = clamp(t, 0.0, 1.0);

    half3 base = rampColor(t);

    // Position along the perpendicular axis, normalised 0...1.
    float halfPerp = 0.5 * (abs(perp.x) * size.x + abs(perp.y) * size.y);
    float s = 0.5 + dot(position - centre, perp) / (2.0 * halfPerp);
    // Zero-mean luminance highlight: top-right brighter, bottom-left darker.
    // It also drifts with tilt so the sheen travels across the card.
    float sShift = s + tilt.y * 0.18 - tilt.x * 0.12;
    float perpHighlight = (0.5 - sShift) * 0.12;     // ~ +-0.06 (=+-15/255)
    base = clamp(base + half3(half(perpHighlight)), half3(0.0), half3(1.0));

    // Monochrome film grain: crisp per-cell speckle, stronger on mid-tones
    // and fading on the whites. Cells are slightly sub-point so the speckle
    // reads finer, the amplitude is light, and it leans bright (lighter
    // specks rather than dark) by biasing the noise above zero.
    float n = hash21(floor(position / 0.8)) - 0.4;
    float luma = dot(base, half3(0.2126h, 0.7152h, 0.0722h));
    float grainAmp = 0.0855 * (1.0 - 0.45 * float(luma));
    base = clamp(base + half3(half(n * grainAmp)), half3(0.0), half3(1.0));

    return half4(base, 1.0);
}
