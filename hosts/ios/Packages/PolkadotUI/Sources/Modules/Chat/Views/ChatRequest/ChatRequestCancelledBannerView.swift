import DesignSystem
import SwiftUI
import FoundationExt

public struct ChatRequestCancelledBannerView: View {
    let viewModel: ViewModel

    public var body: some View {
        Text(
            getMessage(for: viewModel.username)
        )
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .padding(.top, DSSpacings.mediumIncreased)
        .padding(.bottom, DSSpacings.large)
        .background(Color.bgSurfaceContainer)
    }
}

public extension ChatRequestCancelledBannerView {
    struct ViewModel {
        let username: String

        public init(username: String) {
            self.username = username
        }
    }
}

private extension ChatRequestCancelledBannerView {
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
                String(localized: .chatRequestCancelledBannerMessage(username: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
        .toAttributedStringOrEmpty()
    }
}

#Preview(traits: .sizeThatFitsLayout) {
    UIHostingConfiguration {
        ChatRequestCancelledBannerView(
            viewModel: .init(username: "Maxwell.42")
        )
        .background(Color.bgSurfaceContainer)
    }
    .makeContentView()
}
