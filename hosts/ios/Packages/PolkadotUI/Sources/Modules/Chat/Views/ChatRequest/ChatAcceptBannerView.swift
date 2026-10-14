import DesignSystem
import ExternalAccessibility
import SwiftUI
import FoundationExt

public struct ChatAcceptBannerView: View {
    @State var viewModel: ViewModel

    init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(alignment: .center, spacing: 0) {
            Text(getTitle(for: viewModel.username))
                .multilineTextAlignment(.center)

            Text(
                getSubtitle(for: viewModel.username)
            )
            .multilineTextAlignment(.center)
            .padding(.top, 8)

            HStack(alignment: .center, spacing: 8) {
                Button(action: viewModel.onDecline) {
                    Text(.commonDecline)
                        .typography(.titleSmall)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.destructiveDark44)

                Button(action: viewModel.onAccept) {
                    Text(.commonAccept)
                        .typography(.titleSmall)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.mainDark44)
                .accessibilityId(AccessibilityID.Chat.acceptRequestButton)
            }
            .padding(.top, 16)
        }
        .padding(.top, 16)
        .padding(.horizontal, 16)
        .padding(.bottom, 0)
        .background(Color.bgSurfaceContainer)
    }
}

public extension ChatAcceptBannerView {
    struct ViewModel {
        let username: String
        let onDecline: () -> Void
        let onAccept: () -> Void

        public init(
            username: String,
            onDecline: @escaping () -> Void,
            onAccept: @escaping () -> Void
        ) {
            self.username = username
            self.onDecline = onDecline
            self.onAccept = onAccept
        }
    }
}

private extension ChatAcceptBannerView {
    func getTitle(for username: String) -> AttributedString {
        let highlightingAttributes = LabelStyle.body14SemiBold().attributes(
            for: .center,
            textColor: UIColor.fgPrimary
        )

        let defaultAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: UIColor.fgPrimary
        )

        return NSAttributedString.highlightedItems(
            [username],
            formattingClosure: { items in
                String(localized: .chatRequestBannerTitle(username: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
        .toAttributedStringOrEmpty()
    }

    func getSubtitle(for username: String) -> AttributedString {
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
                String(localized: .chatRequestBannerSubtitle(username: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
        .toAttributedStringOrEmpty()
    }
}

#Preview(traits: .sizeThatFitsLayout) {
    UIHostingConfiguration {
        ChatAcceptBannerView(
            viewModel: .init(
                username: "Maxwell.42",
                onDecline: {},
                onAccept: {}
            )
        )
    }
    .makeContentView()
}
