import DesignSystem
import SwiftUI
import FoundationExt

public struct ChatRequestedBannerView: View {
    @State var viewModel: ViewModel

    init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        Text(
            getMessage(for: viewModel.username)
        )
        .multilineTextAlignment(.center)
        .padding(.horizontal, 16)
        .padding(.top, 0)
        .padding(.bottom, 24)
    }
}

public extension ChatRequestedBannerView {
    struct ViewModel {
        let username: String

        public init(
            username: String
        ) {
            self.username = username
        }
    }
}

private extension ChatRequestedBannerView {
    func getMessage(for username: String) -> AttributedString {
        let defaultAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: UIColor.fgTertiary
        )

        let highlightingAttributes = LabelStyle.body14SemiBold().attributes(
            for: .center,
            textColor: UIColor.fgTertiary
        )

        return NSAttributedString.highlightedItems(
            [username],
            formattingClosure: { items in
                String(localized: .chatRequestedBannerMessage(username: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
        .toAttributedStringOrEmpty()
    }
}

#Preview(traits: .sizeThatFitsLayout) {
    UIHostingConfiguration {
        ChatRequestedBannerView(
            viewModel: .init(username: "Maxwell.42")
        )
        .background(Color.bgSurfaceContainer)
    }
    .makeContentView()
}
