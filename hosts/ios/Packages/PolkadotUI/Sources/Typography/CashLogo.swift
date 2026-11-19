import DesignSystem
import SwiftUI
import UIKit

public enum CashLogo {
    public static let ticker = "CASH"

    public enum Wordmark {
        case capital
        case smallCaps

        public var glyph: String {
            switch self {
            case .capital: "C"
            case .smallCaps: "c"
            }
        }
    }

    public static func draws(_ symbol: String) -> Bool {
        symbol == ticker
    }

    public static func font(size: CGFloat) -> UIFont {
        UIFont.registerAllFontsIfNeeded()
        return UIFont(name: fontName, size: size) ?? .systemFont(ofSize: size)
    }

    public static func unit(_ symbol: String, wordmark: Wordmark = .smallCaps, fallback: UIFont) -> NSAttributedString {
        guard draws(symbol) else {
            return NSAttributedString(string: symbol, attributes: [.font: fallback])
        }

        return NSAttributedString(string: wordmark.glyph, attributes: [.font: font(size: fallback.pointSize)])
    }

    public static func styled(_ text: String, symbol: String, font: UIFont) -> NSAttributedString {
        let result = NSMutableAttributedString(string: text, attributes: [.font: font])
        guard draws(symbol) else { return result }

        for range in tickerRanges(in: text).reversed() {
            let wordmark = opensSentence(text, at: range.lowerBound) ? Wordmark.capital : .smallCaps
            result.replaceCharacters(
                in: NSRange(range, in: text),
                with: NSAttributedString(string: wordmark.glyph, attributes: [.font: Self.font(size: font.pointSize)])
            )
        }
        return result
    }

    public static func styled(_ text: String, symbol: String, typography: TypographyStyle) -> AttributedString {
        AttributedString(styled(text, symbol: symbol, font: UIFont.app(typography)))
    }
}

private extension CashLogo {
    static let fontName = "CashLogo-Regular"

    static let tickerPattern = try? Regex("\\b\(ticker)\\b")

    static func tickerRanges(in text: String) -> [Range<String.Index>] {
        guard let tickerPattern else { return [] }
        return text.ranges(of: tickerPattern)
    }

    static func opensSentence(_ text: String, at index: String.Index) -> Bool {
        guard let last = text[..<index].last(where: { !$0.isWhitespace }) else { return true }
        return ".!?".contains(last)
    }
}

public struct DSTicker: View {
    private let symbol: String
    private let wordmark: CashLogo.Wordmark
    private let typography: TypographyStyle

    public init(_ symbol: String, wordmark: CashLogo.Wordmark = .smallCaps, typography: TypographyStyle) {
        self.symbol = symbol
        self.wordmark = wordmark
        self.typography = typography
    }

    public var body: some View {
        if CashLogo.draws(symbol) {
            Text(wordmark.glyph)
                .font(Font(CashLogo.font(size: typography.resolvedSpec.size)))
                .accessibilityLabel(symbol)
        } else {
            Text(symbol).typography(typography)
        }
    }
}
