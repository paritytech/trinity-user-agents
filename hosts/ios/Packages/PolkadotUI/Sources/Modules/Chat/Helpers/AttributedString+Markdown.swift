import SwiftUI
import UIKit

/// One detector for the chat. A detector is immutable and safe on any thread, and it is slow to make, and the chat
/// screen draws each text message again at each list update.
private let linkDetector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

extension AttributedString {
    /// The text with no change, and a link on each web address that the data detector finds: an `https` or
    /// `http` address, or a bare domain. This is the `Text` body of a product (02-DESIGN §6.1).
    static func plainText(linkingWeb text: String) -> Self {
        ChatAttributedTextCache.shared.attributedString(for: text, style: "plain-web") {
            var attributed = AttributedString(text)
            guard let detector = linkDetector else {
                return attributed
            }

            let range = NSRange(text.startIndex ..< text.endIndex, in: text)
            for match in detector.matches(in: text, options: [], range: range) {
                guard
                    let url = webLink(of: match, in: text),
                    let textRange = Range(match.range, in: text),
                    let lower = AttributedString.Index(textRange.lowerBound, within: attributed),
                    let upper = AttributedString.Index(textRange.upperBound, within: attributed)
                else {
                    continue
                }

                attributed[lower ..< upper].link = url
            }
            return attributed
        }
    }

    /// The link of a data detector match in a chat text: an `https` or `http` address as it is written, and a bare
    /// domain (`www.example.com`, `example.com`) as `https`. Other links, for example a mail address, are text.
    static func webLink(of match: NSTextCheckingResult, in text: String) -> URL? {
        guard
            let url = match.url,
            let scheme = url.scheme?.lowercased(),
            ["http", "https"].contains(scheme),
            let range = Range(match.range, in: text)
        else {
            return nil
        }

        let written = text[range].lowercased()
        guard !written.hasPrefix("http://"), !written.hasPrefix("https://") else {
            return url
        }

        var components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        components?.scheme = "https"
        return components?.url
    }

    /// The text with a link on each web address that has no link of its own, out of inline code. A Markdown text
    /// writes its links in brackets, but a person also writes a bare address.
    static func addingWebLinks(to attributed: AttributedString) -> Self {
        guard let detector = linkDetector else {
            return attributed
        }

        var linked = attributed
        let text = String(attributed.characters)
        let range = NSRange(text.startIndex ..< text.endIndex, in: text)
        for match in detector.matches(in: text, options: [], range: range) {
            guard
                let url = webLink(of: match, in: text),
                let textRange = Range(match.range, in: text),
                let lower = AttributedString.Index(textRange.lowerBound, within: linked),
                let upper = AttributedString.Index(textRange.upperBound, within: linked)
            else {
                continue
            }

            // The Markdown parser links a `www.` address as `http`. That link is the same address as the match.
            let runs = linked[lower ..< upper].runs
            let isFree = runs.allSatisfy { run in
                (run.link == nil || run.link == match.url) && run.inlinePresentationIntent?.contains(.code) != true
            }
            if isFree {
                linked[lower ..< upper].link = url
            }
        }
        return linked
    }

    static func from(markdown text: String, textColor: UIColor?) -> Self {
        ChatAttributedTextCache.shared.attributedString(
            for: text,
            style: "inline:\(textColor.map { String(describing: $0) } ?? "none")"
        ) {
            from(markdown: text, textColor: textColor.map { Color($0) })
        }
    }

    static func from(markdown text: String, textColor: Color? = nil) -> Self {
        // 1. Parse Markdown first
        var attributed = (try? AttributedString(
            markdown: text,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(text)

        // 2. Apply Base Color
        if let textColor {
            attributed.foregroundColor = Color(textColor)
        }

        // 3. Detect Raw Links
        guard let detector = linkDetector else {
            return attributed
        }
        let plainText = String(attributed.characters[...])

        let matches = detector.matches(
            in: plainText,
            options: [],
            range: NSRange(plainText.startIndex ..< plainText.endIndex, in: plainText)
        )

        for match in matches {
            guard
                let range = Range(match.range, in: plainText),
                let url = match.url
            else {
                continue
            }

            let distLower = plainText.distance(from: plainText.startIndex, to: range.lowerBound)
            let distUpper = plainText.distance(from: plainText.startIndex, to: range.upperBound)

            let characters = attributed.characters
            let start = characters.index(characters.startIndex, offsetBy: distLower)
            let end = characters.index(characters.startIndex, offsetBy: distUpper)
            let attrRange = start ..< end

            guard attributed[attrRange].link == nil else {
                continue
            }
            attributed[attrRange].link = url
        }

        let linkRanges = attributed.runs.compactMap { run in
            run.link == nil ? nil : run.range
        }
        linkRanges.forEach {
            attributed[$0].foregroundColor = .blue
            attributed[$0].underlineStyle = .single
        }

        return attributed
    }
}
