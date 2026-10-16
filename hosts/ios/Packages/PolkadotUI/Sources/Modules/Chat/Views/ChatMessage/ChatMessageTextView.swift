import SwiftUI
import DesignSystem

public struct ChatMessageTextView: View, Hashable {
    public static let reuseIdentifier = "ChatMessageTextView"

    public let viewModel: ViewModel

    public init(viewModel: ViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
            styledText
                .foregroundStyle(Color(viewModel.textColor))
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .environment(\.openURL, OpenURLAction(handler: openLinkAction(viewModel.openLink)))

            if let collapse = viewModel.collapse {
                ChatShowMoreButton(title: collapse.title, color: Color(viewModel.textColor), action: collapse.onExpand)
            }
        }
    }

    /// The text of a product follows the text size that the user picks (P7.2). The other texts keep the size
    /// of the design system.
    @ViewBuilder private var styledText: some View {
        if viewModel.scalesWithTextSize {
            (Text(viewModel.attributedText) + statusPlaceholder).scaledTypography(.paragraphLarge)
        } else {
            (Text(viewModel.attributedText) + statusPlaceholder).typography(.paragraphLarge)
        }
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
        /// Opens a tapped link. Without it, the system opens the link.
        let openLink: ((URL) -> Void)?
        /// The text follows the text size that the user picks. Only the texts of products do.
        let scalesWithTextSize: Bool
        /// The "Show more" button of a long text that shows only its start. Nil for a text that shows in full.
        let collapse: ChatTextCollapse?

        public init(
            text: String,
            textColor: UIColor,
            statusPlaceholderImage: UIImage? = nil
        ) {
            rawText = text
            self.textColor = textColor
            attributedText = .from(markdown: text, textColor: textColor)
            self.statusPlaceholderImage = statusPlaceholderImage
            openLink = nil
            scalesWithTextSize = false
            collapse = nil
        }

        /// Product rich text: the Markdown subset of ``ChatMarkdownText``. A text longer than
        /// ``ChatTextCollapse/collapsedLength`` shows only its start when `collapse` is not nil.
        public init(
            markdown: String,
            textColor: UIColor,
            statusPlaceholderImage: UIImage? = nil,
            openLink: ((URL) -> Void)?,
            collapse: ChatTextCollapse? = nil
        ) {
            rawText = markdown
            self.textColor = textColor
            let full = ChatMarkdownText.attributedString(from: markdown)
            let start = collapse.flatMap { _ in ChatTextCollapse.collapsed(full) }
            attributedText = start ?? full
            self.statusPlaceholderImage = statusPlaceholderImage
            self.openLink = openLink
            scalesWithTextSize = true
            self.collapse = start == nil ? nil : collapse
        }

        /// The `Text` body of a product (02-DESIGN §6.1): plain text with no Markdown, and its `https` links. A
        /// tapped link opens through `openLink`. A text longer than ``ChatTextCollapse/collapsedLength`` shows only
        /// its start when `collapse` is not nil.
        public init(
            plainText text: String,
            textColor: UIColor,
            statusPlaceholderImage: UIImage? = nil,
            openLink: ((URL) -> Void)?,
            collapse: ChatTextCollapse? = nil
        ) {
            rawText = text
            self.textColor = textColor
            let full = AttributedString.plainText(linkingHTTPS: text)
            let start = collapse.flatMap { _ in ChatTextCollapse.collapsed(full) }
            attributedText = start ?? full
            self.statusPlaceholderImage = statusPlaceholderImage
            self.openLink = openLink
            scalesWithTextSize = true
            self.collapse = start == nil ? nil : collapse
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
            openLink = nil
            scalesWithTextSize = false
            collapse = nil
        }

        public static func == (lhs: ViewModel, rhs: ViewModel) -> Bool {
            lhs.attributedText == rhs.attributedText &&
                lhs.rawText == rhs.rawText &&
                lhs.textColor == rhs.textColor &&
                lhs.statusPlaceholderImage == rhs.statusPlaceholderImage &&
                lhs.scalesWithTextSize == rhs.scalesWithTextSize &&
                lhs.collapse?.title == rhs.collapse?.title
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(attributedText)
            hasher.combine(rawText)
            hasher.combine(textColor)
            hasher.combine(statusPlaceholderImage)
            hasher.combine(scalesWithTextSize)
            hasher.combine(collapse?.title)
        }

        /// True when the text shows only its start.
        public var isCollapsed: Bool {
            collapse != nil
        }

        /// The characters that the text shows.
        public var shownText: String {
            String(attributedText.characters)
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
