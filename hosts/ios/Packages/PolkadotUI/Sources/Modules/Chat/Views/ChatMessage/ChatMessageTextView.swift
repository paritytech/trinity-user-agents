import SwiftUI
import DesignSystem

public struct ChatMessageTextView: View, Hashable {
    public static let reuseIdentifier = "ChatMessageTextView"

    public let viewModel: ViewModel

    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        (Text(viewModel.attributedText) + statusPlaceholder)
            .typography(.paragraphLarge)
            .foregroundStyle(Color(viewModel.textColor))
            .multilineTextAlignment(.leading)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var statusPlaceholder: Text {
        guard let image = viewModel.statusPlaceholderImage else {
            return Text(verbatim: "")
        }
        return Text(Image(uiImage: image))
            .baselineOffset(-6)
    }
}

// MARK: - ViewModel

public extension ChatMessageTextView {
    struct ViewModel: Hashable {
        let attributedText: AttributedString
        let rawText: String
        let textColor: UIColor
        let statusPlaceholderImage: UIImage?

        public init(
            text: String,
            textColor: UIColor,
            statusPlaceholderImage: UIImage? = nil
        ) {
            rawText = text
            self.textColor = textColor
            attributedText = .from(markdown: text, textColor: textColor)
            self.statusPlaceholderImage = statusPlaceholderImage
        }

        /// Initializer for pre-built attributed text (e.g., diff highlighting)
        public init(
            attributedText: AttributedString,
            textColor: UIColor = .label,
            statusPlaceholderImage: UIImage? = nil
        ) {
            self.attributedText = attributedText
            rawText = String(attributedText.characters)
            self.textColor = textColor
            self.statusPlaceholderImage = statusPlaceholderImage
        }
    }
}

// MARK: - Previews

#if DEBUG
    #Preview("Markdown & Links") {
        VStack(spacing: 20) {
            // 1. Markdown: Bold + Italic
            ChatMessageTextView(viewModel: .init(
                text: "This is **Bold** and this is *Italic*.",
                textColor: .black
            ))
            .background(Color.gray.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 12))

            // 2. Markdown Link
            ChatMessageTextView(viewModel: .init(
                text: "Click [here](https://apple.com) for Apple.",
                textColor: .black
            ))
            .background(Color.gray.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 12))

            // 3. Mixed: Markdown + Raw Link
            ChatMessageTextView(viewModel: .init(
                text: "Markdown style **bold**, and a raw link: google.com, raw link2: https://www.apple.com",
                textColor: .black
            ))
            .background(Color.gray.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 12))
        }
        .padding()
    }
#endif
