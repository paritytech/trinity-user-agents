//
//  Shine.metal
//  Shining surface with dimming
//

#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

[[ stitchable ]]
half4 shine(float2 position,
            half4 color, // required by colorEffect, not used
            float2 size,
            float2 tilt,
            float strength,
            float dimming,
            float2 spread,
            float center) {
    float2 centre = size * 0.5;

    // Main gradient axis: 151.367deg (screen space, y-down).
    const float2 dir  = float2(0.4787, 0.8780);
    // Perpendicular axis (points down-left), s=0 is the top-right side.
    const float2 perp = float2(-0.8780, 0.4787);

    // Position along the main axis, normalised 0...1 across the box (CSS-style).
    float halfMain = 0.5 * (abs(dir.x) * size.x + abs(dir.y) * size.y);
    float t = 0.5 + dot(position - centre, dir) / (2.0 * halfMain);
    // Tilt slides the shine core along the axis.
    t += tilt.x * 0.16 - tilt.y * 0.10;

    // Position along the perpendicular axis, normalised 0...1, drifting with
    // tilt like the holographic sheen.
    float halfPerp = 0.5 * (abs(perp.x) * size.x + abs(perp.y) * size.y);
    float s = 0.5 + dot(position - centre, perp) / (2.0 * halfPerp);
    s += tilt.y * 0.18 - tilt.x * 0.12;

    // Gaussian falloff from the core resting at `center` on the main axis:
    // spread.x is the width across the band (along the gradient axis),
    // spread.y the length along its diagonal, so the shine reads as a soft
    // diagonal band that fades the further the surface is from its centre.
    float dMain = (t - center) / spread.x;
    float dPerp = (s - 0.5) / spread.y;
    float falloff = dMain * dMain + dPerp * dPerp;
    // Tight core for the white streak.
    // A twice-as-wide glow drives the dimming so darkness eases in gradually
    // with distance from the shine.
    float core = exp(-falloff);
    float glow = exp(-falloff / 4.0);

    // White at the core, black where the shine fades, blended in one
    // premultiplied layer: rgb carries only the white part, alpha covers both
    // so far-from-shine areas darken the content underneath.
    float shineAlpha = core * strength;
    float dimAlpha = (1.0 - glow) * dimming;

    half white = half(shineAlpha);
    half a = half(clamp(shineAlpha + dimAlpha, 0.0, 1.0));
    return half4(white, white, white, a);
}
