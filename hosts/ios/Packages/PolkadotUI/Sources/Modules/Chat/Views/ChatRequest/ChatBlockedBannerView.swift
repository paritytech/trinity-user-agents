import DesignSystem
import SwiftUI
import FoundationExt

public struct ChatBlockedBannerView: View {
    @State var viewModel: ViewModel

    init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(alignment: .center, spacing: 0) {
            Text(getTitle(for: viewModel.username))
                .multilineTextAlignment(.center)

            Button(action: viewModel.onUnblock) {
                Text(.blockedBannerUnblock)
                    .typography(.titleSmall)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.mainDark44)
            .padding(.top, 16)
        }
        .padding(.top, 16)
        .padding(.horizontal, 16)
        .padding(.bottom, 0)
        .background(Color.bgSurfaceContainer)
    }
}

public extension ChatBlockedBannerView {
    struct ViewModel {
        let username: String
        let onUnblock: () -> Void

        public init(
            username: String,
            onUnblock: @escaping () -> Void
        ) {
            self.username = username
            self.onUnblock = onUnblock
        }
    }
}

private extension ChatBlockedBannerView {
    func getTitle(for username: String) -> AttributedString {
        let highlightingAttributes = LabelStyle.body14SemiBold().attributes(
            for: .center,
            textColor: UIColor.fgTertiary
        )

        let defaultAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: UIColor.fgTertiary
        )

        return NSAttributedString.highlightedItems(
            [username],
            formattingClosure: { items in
                String(localized: .blockedBannerTitle(username: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
        .toAttributedStringOrEmpty()
    }
}

#Preview(traits: .sizeThatFitsLayout) {
    UIHostingConfiguration {
        ChatBlockedBannerView(
            viewModel: .init(
                username: "Maxwell.42",
                onUnblock: {}
            )
        )
    }
    .makeContentView()
}
