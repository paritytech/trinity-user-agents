import SwiftUI
import UIKit

/// One detector for the chat. A detector is immutable and safe on any thread, and it is slow to make, and the chat
/// screen draws each text message again at each list update.
private let linkDetector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

extension AttributedString {
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
