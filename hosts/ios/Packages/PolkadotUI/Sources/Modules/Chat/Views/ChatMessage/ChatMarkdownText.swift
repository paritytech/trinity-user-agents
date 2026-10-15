import DesignSystem
import SwiftUI

/// The Markdown subset of product rich text: paragraphs, bold, italic, inline code, fenced code, lists, block
/// quotes and `https` links. A heading shows as a bold paragraph. Raw HTML tags, images, tables and other
/// links show as their text only.
public struct ChatMarkdownText: View, Hashable {
    let markdown: String
    let attributedText: AttributedString
    let onOpenLink: ((URL) -> Void)?

    public init(markdown: String, onOpenLink: ((URL) -> Void)?) {
        self.markdown = markdown
        attributedText = Self.attributedString(from: markdown)
        self.onOpenLink = onOpenLink
    }

    public var body: some View {
        // The text follows the text size that the user picks (P7.2).
        Text(attributedText)
            .scaledTypography(.paragraphLarge)
            .foregroundStyle(.fgPrimary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .environment(\.openURL, OpenURLAction(handler: openLinkAction(onOpenLink)))
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.markdown == rhs.markdown
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(markdown)
    }

    public static func attributedString(from markdown: String) -> AttributedString {
        ChatAttributedTextCache.shared.attributedString(for: markdown, style: "markdown") {
            parse(markdown)
        }
    }

    private static func parse(_ markdown: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            allowsExtendedAttributes: false,
            interpretedSyntax: .full,
            failurePolicy: .returnPartiallyParsedIfPossible
        )

        guard let parsed = try? AttributedString(markdown: markdown, options: options) else {
            return AttributedString(markdown)
        }

        return ChatMarkdownRenderer().render(parsed)
    }
}

/// Opens a link through the host when the host gives a handler, and through the system when it does not.
func openLinkAction(_ onOpenLink: ((URL) -> Void)?) -> (URL) -> OpenURLAction.Result {
    { url in
        guard let onOpenLink else { return .systemAction }
        onOpenLink(url)
        return .handled
    }
}

/// Turns block structure, which `Text` does not draw, into plain characters and inline styles.
private struct ChatMarkdownRenderer {
    private enum Symbols {
        static let bullet = "• "
        static let quote = "▎ "
        static let cellSeparator = " | "
        static let lineBreak = "\n"
        static let blockBreak = "\n\n"
    }

    func render(_ parsed: AttributedString) -> AttributedString {
        var output = AttributedString()
        var previous: PresentationIntent?
        var isFirst = true

        for run in parsed.runs {
            guard let piece = visiblePiece(of: parsed[run.range], inline: run.inlinePresentationIntent) else {
                continue
            }

            let intent = run.presentationIntent
            let isBlockHTML = run.inlinePresentationIntent?.contains(.blockHTML) == true
            if isFirst || isBlockHTML || intent != previous {
                let separator = isFirst ? "" : separator(after: previous, before: intent)
                output += AttributedString(separator + prefix(for: intent, after: previous))
                previous = intent
                isFirst = false
            }

            output += styled(piece, intent: intent)
        }

        return output
    }
}

private extension ChatMarkdownRenderer {
    /// An inline HTML tag shows nothing. A block of HTML shows its text without the tags.
    func visiblePiece(of slice: AttributedSubstring, inline: InlinePresentationIntent?) -> AttributedString? {
        if inline?.contains(.inlineHTML) == true {
            return nil
        }

        guard inline?.contains(.blockHTML) == true else {
            return AttributedString(slice)
        }

        let text = String(slice.characters)
            .replacingOccurrences(of: "<[^>]*>", with: "", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : AttributedString(text)
    }

    func identities(of intent: PresentationIntent?) -> Set<Int> {
        Set(intent?.components.map(\.identity) ?? [])
    }

    func separator(after previous: PresentationIntent?, before next: PresentationIntent?) -> String {
        let previousIdentities = identities(of: previous)
        let shared = next?.components.filter { previousIdentities.contains($0.identity) } ?? []

        if shared.contains(where: { if case .tableRow = $0.kind { true } else { $0.kind == .tableHeaderRow } }) {
            return Symbols.cellSeparator
        }

        let staysInList = shared.contains { component in
            switch component.kind {
            case .orderedList,
                 .unorderedList,
                 .table: true
            default: false
            }
        }

        return staysInList ? Symbols.lineBreak : Symbols.blockBreak
    }

    /// Components run from the innermost block to the outermost one. Only the innermost list item shows
    /// its marker, and only on its first block. Each outer list item adds a blank of the marker's width.
    func prefix(for intent: PresentationIntent?, after previous: PresentationIntent?) -> String {
        guard let components = intent?.components else { return "" }

        let previousIdentities = identities(of: previous)
        let innermostItem = components.firstIndex { if case .listItem = $0.kind { true } else { false } }
        var prefix = ""

        for (index, component) in components.enumerated().reversed() {
            switch component.kind {
            case .blockQuote:
                prefix += Symbols.quote
            case let .listItem(ordinal):
                let parent = components.indices.contains(index + 1) ? components[index + 1].kind : nil
                let marker = parent == .orderedList ? "\(ordinal). " : Symbols.bullet
                let showsMarker = index == innermostItem && !previousIdentities.contains(component.identity)
                prefix += showsMarker ? marker : String(repeating: " ", count: marker.count)
            default:
                break
            }
        }

        return prefix
    }

    func styled(_ piece: AttributedString, intent: PresentationIntent?) -> AttributedString {
        var piece = piece
        piece.presentationIntent = nil
        piece.imageURL = nil

        if let link = piece.link, link.scheme?.lowercased() != "https" {
            piece.link = nil
        }

        let kinds = intent?.components.map(\.kind) ?? []

        if kinds.contains(where: { if case .header = $0 { true } else { false } }) {
            var inline = piece.inlinePresentationIntent ?? []
            inline.insert(.stronglyEmphasized)
            piece.inlinePresentationIntent = inline
        }

        if kinds.contains(where: { if case .codeBlock = $0 { true } else { false } }) {
            piece.inlinePresentationIntent = .code
            while piece.characters.last == "\n" {
                piece.characters.removeLast()
            }
        }

        return piece
    }
}
