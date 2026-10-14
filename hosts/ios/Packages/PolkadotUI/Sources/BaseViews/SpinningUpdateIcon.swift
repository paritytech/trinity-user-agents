import SwiftUI

public struct SpinningUpdateIcon: View {
    @State private var isAnimating = false

    public init() {}

    public var body: some View {
        Image(.iconRefreshing)
            .renderingMode(.template)
            .rotationEffect(Angle(degrees: isAnimating ? -360 : 0))
            .onAppear {
                withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) {
                    isAnimating = true
                }
            }
    }
}
