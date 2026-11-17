import DesignSystem
import SwiftUI
import FoundationExt

public struct ChatRequestedBannerView: View {
    @State var viewModel: ViewModel

    init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(alignment: .center, spacing: DSSpacings.mediumIncreased) {
            Text(
                getMessage(for: viewModel.username)
            )
            .multilineTextAlignment(.center)

            Button(action: viewModel.onCancel) {
                Text(.chatRequestedBannerCancel)
                    .typography(.titleSmall)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.destructiveDark44)
        }
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .padding(.top, DSSpacings.mediumIncreased)
        .padding(.bottom, DSSpacings.large)
        .background(Color.bgSurfaceContainer)
    }
}

public extension ChatRequestedBannerView {
    struct ViewModel {
        let username: String
        let onCancel: () -> Void

        public init(
            username: String,
            onCancel: @escaping () -> Void
        ) {
            self.username = username
            self.onCancel = onCancel
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
            viewModel: .init(username: "Maxwell.42", onCancel: {})
        )
        .background(Color.bgSurfaceContainer)
    }
    .makeContentView()
}
