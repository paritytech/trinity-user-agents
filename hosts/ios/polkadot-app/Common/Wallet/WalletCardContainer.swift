import SwiftUI
import PolkadotUI

struct WalletCardContainer<Content: View>: View {
    let color: Color
    let contentPadding: CGFloat

    @ViewBuilder
    let content: () -> Content

    init(
        color: Color,
        contentPadding: CGFloat = 24,
        content: @escaping () -> Content
    ) {
        self.color = color
        self.contentPadding = contentPadding
        self.content = content
    }

    var body: some View {
        content()
            .padding(contentPadding)
            .frame(maxWidth: .infinity)
            .background(color)
            .clipShape(RoundedRectangle(cornerRadius: 24))
    }
}
