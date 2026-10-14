import SwiftUI

// Uses the motion shine as a reveal instead of a highlight: the view rests at
// `baseOpacity` and rises to full opacity where the gyroscope-driven shine
// core passes over it. Only the shader's alpha is used, so no white tint is
// added.

public extension View {
    func motionShineReveal(
        _ parameters: MotionShineParameters,
        baseOpacity: Double,
        isActive: Bool = true,
        lagged: Bool = false
    ) -> some View {
        modifier(
            MotionShineRevealModifier(
                parameters: parameters,
                baseOpacity: baseOpacity,
                isActive: isActive,
                lagged: lagged
            )
        )
    }
}

struct MotionShineRevealModifier: ViewModifier {
    let parameters: MotionShineParameters
    let baseOpacity: Double
    let isActive: Bool
    let lagged: Bool

    private var motion: CardEffectMotionEngine { .shared }

    func body(content: Content) -> some View {
        if isActive {
            content
                .opacity(baseOpacity)
                .overlay {
                    content
                        .mask {
                            ShaderFillView(
                                tilt: lagged ? motion.delayedTilt : motion.tilt,
                                shader: ShineShaders.whiteShine(parameters: parameters)
                            )
                        }
                        .allowsHitTesting(false)
                }
                .onAppear { motion.retain() }
                .onDisappear { motion.release() }
        } else {
            content.opacity(baseOpacity)
        }
    }
}
