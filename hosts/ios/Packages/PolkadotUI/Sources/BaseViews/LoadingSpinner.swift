import SwiftUI
import DesignSystem

public struct LoadingSpinner: View {
    let lineWidth: CGFloat
    let strokeStyle: AnyShapeStyle

    @State private var isAnimating = false

    public init(
        lineWidth: CGFloat = 5,
        strokeStyle: any ShapeStyle = .fgPrimary
    ) {
        self.lineWidth = lineWidth
        self.strokeStyle = AnyShapeStyle(strokeStyle)
    }

    public var body: some View {
        Circle()
            .inset(by: lineWidth / 2)
            .trim(from: 0.0, to: 0.6)
            .stroke(
                strokeStyle,
                style: StrokeStyle(
                    lineWidth: lineWidth,
                    lineCap: .round
                )
            )
            .rotationEffect(.degrees(isAnimating ? 360 : 0))
            .animation(
                .timingCurve(0.3, 0.4, 1, 1, duration: 1.5)
                    .repeatForever(autoreverses: false),
                value: isAnimating
            )
            .onAppear {
                isAnimating = true
            }
    }
}
