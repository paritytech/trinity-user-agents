import UIKit

public extension UILabel {
    func applyAttributedText(
        _ text: String,
        lineHeight: CGFloat,
        alignment: NSTextAlignment = .left
    ) {
        let attributedString = NSMutableAttributedString.createFromString(
            text,
            lineHeight: lineHeight,
            alignment: alignment
        )

        attributedText = attributedString
    }
}

extension NSMutableAttributedString {
    static func createFromString(
        _ string: String,
        lineHeight: CGFloat,
        alignment: NSTextAlignment
    ) -> NSMutableAttributedString {
        let attributedString = NSMutableAttributedString(string: string)
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = lineHeight
        style.alignment = alignment

        attributedString.addAttribute(
            .paragraphStyle,
            value: style,
            range: NSRange(location: 0, length: attributedString.length)
        )

        return attributedString
    }
}
