import SwiftUI

public struct DSWalletCardContainer<Content: View>: View {
    @ViewBuilder
    let content: () -> Content

    public init(content: @escaping () -> Content) {
        self.content = content
    }

    public var body: some View {
        content()
            .padding(0.5)
            .frame(maxWidth: .infinity)
            .bordered(
                cornerRadius: 24,
                gradient: LinearGradient(
                    colors: [
                        Color(hex: 0xEFEDED),
                        Color(hex: 0xB1B0AD)
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
    }
}
