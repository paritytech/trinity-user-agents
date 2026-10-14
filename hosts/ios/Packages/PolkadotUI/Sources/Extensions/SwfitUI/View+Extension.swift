import SwiftUI
import DesignSystem

private struct BorderModifier: ViewModifier {
    let cornerRadius: CGFloat
    let borderColor: Color

    func body(content: Content) -> some View {
        content
            .cornerRadius(24)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(borderColor, lineWidth: 1)
            )
    }
}

private struct GradientBorderModifier: ViewModifier {
    let width: CGFloat
    let cornerRadius: CGFloat
    let gradient: LinearGradient

    func body(content: Content) -> some View {
        content
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(gradient, lineWidth: width)
            )
    }
}

public extension View {
    func bordered(
        cornerRadius: CGFloat = 16,
        color: Color
    ) -> some View {
        modifier(BorderModifier(cornerRadius: cornerRadius, borderColor: color))
    }

    func bordered(
        width: CGFloat = 1,
        cornerRadius: CGFloat = 16,
        gradient: LinearGradient
    ) -> some View {
        modifier(GradientBorderModifier(width: width, cornerRadius: cornerRadius, gradient: gradient))
    }
}

// MARK: Shimmering

private struct ShimmerModifier: ViewModifier {
    @State private var phase: CGFloat = -0.3
    let color: Color

    func body(content: Content) -> some View {
        content
            .overlay(
                LinearGradient(
                    stops: [
                        .init(color: .clear, location: 0),
                        .init(color: color.opacity(0.5), location: 0.5),
                        .init(color: .clear, location: 1)
                    ],
                    startPoint: UnitPoint(x: phase - 0.3, y: 0.5),
                    endPoint: UnitPoint(x: phase + 0.3, y: 0.5)
                )
                .allowsHitTesting(false)
            )
            .mask(content)
            .onAppear {
                withAnimation(.linear(duration: 1.2).repeatForever(autoreverses: false)) {
                    phase = 1.3
                }
            }
    }
}

public extension View {
    @ViewBuilder
    func shimmering(active: Bool) -> some View {
        if active {
            modifier(ShimmerModifier(color: .black))
        } else {
            self
        }
    }
}

// MARK: - Aspect ratio

private struct AspectRatioModifier: ViewModifier {
    let ratio: CGFloat?

    func body(content: Content) -> some View {
        if let ratio {
            content.aspectRatio(ratio, contentMode: .fit)
        } else {
            content
        }
    }
}

public extension View {
    func cardAspectRatio() -> some View {
        modifier(AspectRatioModifier(ratio: 1.71))
    }
}
