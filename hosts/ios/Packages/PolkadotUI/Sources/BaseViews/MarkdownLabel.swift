import UIKit

class MarkdownLabel: Label {
    private var rawText: String?

    override var text: String? {
        get { rawText }
        set {
            rawText = newValue
            guard let newValue else {
                attributedText = nil
                super.text = nil
                return
            }
            setText(markdown: newValue)
        }
    }

    func setText(markdown: String) {
        rawText = markdown
        let attributed = AttributedString.from(markdown: markdown, textColor: textColor)
        let nsAttributed = NSMutableAttributedString(attributed)
        applyBaseStyle(in: nsAttributed)
        attributedText = nsAttributed
    }

    private func applyBaseStyle(in attributed: NSMutableAttributedString) {
        guard let style else { return }
        let fullRange = NSRange(location: 0, length: attributed.length)

        let attributes = style.attributes(
            for: textAlignment,
            lineBreakMode: lineBreakMode,
            textColor: textColor
        )
        attributed.addAttributes(attributes, range: fullRange)
    }
}
