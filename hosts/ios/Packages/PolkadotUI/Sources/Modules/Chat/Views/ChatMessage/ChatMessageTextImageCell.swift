import DesignSystem
import ExternalAccessibility
import SwiftUI

public struct ChatMessageTextImageCell: View, Hashable {
    public static let reuseIdentifier = "ChatMessageTextImageCell"

    public let viewModel: ViewModel
    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(spacing: 16) {
            Image(uiImage: viewModel.image)
                .resizable()
                .modifier(NaturalAspectIfNeeded(
                    enabled: viewModel.lockFullWidth,
                    fallbackAspectRatio: viewModel.aspectRatio
                ))
                .frame(maxWidth: .infinity, maxHeight: viewModel.lockFullWidth ? 280 : 400)

            if let text = viewModel.text {
                Text(LocalizedStringKey(text))
                    .foregroundStyle(Color.fgPrimary)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .modifier(LockFullWidthModifier(enabled: viewModel.lockFullWidth))
        .accessibilityId(rawValue: viewModel.accessibilityId)
    }
}

private struct NaturalAspectIfNeeded: ViewModifier {
    let enabled: Bool
    let fallbackAspectRatio: CGFloat

    func body(content: Content) -> some View {
        if enabled {
            content.aspectRatio(contentMode: .fit)
        } else {
            content.aspectRatio(fallbackAspectRatio, contentMode: .fit)
        }
    }
}

private struct LockFullWidthModifier: ViewModifier {
    let enabled: Bool

    func body(content: Content) -> some View {
        if enabled {
            content.frame(idealWidth: .infinity, maxWidth: .infinity, alignment: .leading)
        } else {
            content
        }
    }
}

public extension ChatMessageTextImageCell {
    struct ViewModel: Hashable {
        /// Can be markdown text
        let text: String?
        let image: UIImage
        let aspectRatio: CGFloat
        let lockFullWidth: Bool
        let accessibilityId: String?

        public init(
            text: String?,
            image: UIImage,
            aspectRatio: CGFloat,
            lockFullWidth: Bool = false,
            accessibilityId: (any AccessibilityIdentifying)? = nil
        ) {
            self.text = text
            self.image = image
            self.aspectRatio = aspectRatio
            self.lockFullWidth = lockFullWidth
            self.accessibilityId = accessibilityId?.rawValue
        }
    }
}

#if DEBUG
    #Preview(traits: .sizeThatFitsLayout) {
        let imageText1_5 = ChatMessageTextImageCell.ViewModel(
            text: """
            Option 2. **Send a payment in chat.**
            **One and done.**

            ✍️ Pick a username, enter an amount and confirm. The payment settles on chain.
            """,
            image: UIImage(systemName: "1.circle.fill")!,
            aspectRatio: 1.5
        )

        let imageText1_0 = ChatMessageTextImageCell.ViewModel(
            text: "This image has an aspect ratio of 1.0 (square).",
            image: UIImage(systemName: "2.circle.fill")!,
            aspectRatio: 1.0
        )

        let imageOnly = ChatMessageTextImageCell.ViewModel(
            text: nil,
            image: UIImage(systemName: "3.circle.fill")!,
            aspectRatio: 1
        )

        VStack(spacing: 20) {
            ChatMessageTextImageCell(viewModel: imageText1_5)
            ChatMessageTextImageCell(viewModel: imageText1_0)
            ChatMessageTextImageCell(viewModel: imageOnly)
        }
        .background(.red)
    }
#endif
