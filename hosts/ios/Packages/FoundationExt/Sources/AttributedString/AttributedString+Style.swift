import UIKit

public extension NSAttributedString {
    static func coloredItems(
        _ items: [String],
        formattingClosure: ([String]) -> String,
        color: UIColor
    ) -> NSAttributedString {
        highlightedItems(
            items,
            formattingClosure: formattingClosure,
            highlightingAttributes: [.foregroundColor: color],
            defaultAttributes: nil
        )
    }

    static func coloredSubstring(
        _ substring: String,
        in string: String,
        with color: UIColor
    ) -> NSAttributedString {
        let attributedString = NSAttributedString(string: string)
        let highlightAttributes: [NSAttributedString.Key: Any] = [.foregroundColor: color]
        let decorator = HighlightingAttributedStringDecorator(
            pattern: substring,
            attributes: highlightAttributes
        )
        return decorator.decorate(attributedString: attributedString)
    }

    static func highlightedItems(
        _ items: [String],
        formattingClosure: ([String]) -> String,
        highlightingAttributes: [NSAttributedString.Key: Any],
        defaultAttributes: [NSAttributedString.Key: Any]?,
        customAttributes: [Int: [NSAttributedString.Key: Any]] = [:]
    ) -> NSAttributedString {
        let marker = AttributedReplacementStringDecorator.marker
        let decorator = AttributedReplacementStringDecorator(
            pattern: marker,
            replacements: items,
            attributes: highlightingAttributes
        )

        customAttributes.forEach { key, attributes in
            decorator.addCustomAttributes(for: key, attributes: attributes)
        }

        let markers = Array(repeating: marker, count: items.count)
        let template = formattingClosure(markers)

        let attributedString = NSAttributedString(string: template, attributes: defaultAttributes)

        return decorator.decorate(attributedString: attributedString)
    }

    static func coloredFontItems(
        _ items: [String],
        formattingClosure: ([String]) -> String,
        color: UIColor,
        font: UIFont
    ) -> NSAttributedString {
        highlightedItems(
            items,
            formattingClosure: formattingClosure,
            highlightingAttributes: [
                .foregroundColor: color,
                .font: font
            ],
            defaultAttributes: nil
        )
    }
}
