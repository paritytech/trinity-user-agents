import SwiftUI

// Overlays a gyroscope-driven light effect on any view: a white shine core
// with the same angle and tilt response as the holographic gradient, dimming
// to black the further the surface is from the core:
//
//   AssetDetailsBalanceCard(...).motionShine(.balanceCard)
//

public extension View {
    func motionShine(
        _ parameters: MotionShineParameters,
        isActive: Bool = true,
        lagged: Bool = false
    ) -> some View {
        modifier(
            MotionShineModifier(
                parameters: parameters,
                isActive: isActive,
                lagged: lagged
            )
        )
    }
}

struct MotionShineModifier: ViewModifier {
    let parameters: MotionShineParameters
    let isActive: Bool
    let lagged: Bool

    private var motion: CardEffectMotionEngine { .shared }

    func body(content: Content) -> some View {
        if isActive {
            content
                .overlay {
                    ShaderFillView(
                        tilt: lagged ? motion.delayedTilt : motion.tilt,
                        shader: ShineShaders.whiteShine(parameters: parameters)
                    )
                    // Confine the light to the content's own pixels so
                    // transparent areas stay untouched.
                    .mask { content }
                    .allowsHitTesting(false)
                }
                .onAppear { motion.retain() }
                .onDisappear { motion.release() }
        } else {
            content
        }
    }
}

public enum ShineShaders {
    public static func whiteShine(
        parameters: MotionShineParameters
    ) -> (CGSize, CGPoint) -> Shader {
        { size, tilt in
            ShaderLibrary.bundle(.module).shine(
                .float2(Float(size.width), Float(size.height)),
                .float2(Float(tilt.x), Float(tilt.y)),
                .float(Float(parameters.intensity)),
                .float(Float(parameters.dimming)),
                .float2(Float(parameters.width), Float(parameters.length)),
                .float(Float(parameters.center))
            )
        }
    }
}
