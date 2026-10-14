import SwiftUI

public extension Text {
    func textStyle(_ style: LabelStyle) -> Text {
        let text = font(Font(style.font))
            .tracking(style.tracking)

        guard let lineHeight = style.lineHeight else {
            return text
        }

        let fontMetrics = style.fontMetrics
        let scaledLineHeight = fontMetrics?.scaledValue(for: lineHeight) ?? lineHeight

        // We use baselineOffset instead of lineSpacing because baselineOffset returns 'Text',
        // which is necessary for Text concatenation.
        let baselineOffset = (scaledLineHeight - style.font.lineHeight) / 4.0
        return text.baselineOffset(baselineOffset)
    }
}
