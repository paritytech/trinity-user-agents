import Foundation

public enum SingleEmojiChecker {
    public static func isSingleEmoji(_ text: String) -> Bool {
        guard !text.isEmpty else { return false }

        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)

        guard trimmed.count == 1 else { return false }
        guard let scalar = trimmed.unicodeScalars.first else { return false }

        let isEmoji = scalar.properties.isEmoji
        let isEmojiPresentation = scalar.properties.isEmojiPresentation
        let isEmojiModifier = scalar.properties.isEmojiModifier
        let isEmojiModifierBase = scalar.properties.isEmojiModifierBase

        if isEmojiPresentation || isEmojiModifier || isEmojiModifierBase {
            return true
        }

        guard isEmoji else {
            return false
        }

        let scalarValue = scalar.value

        if (scalarValue >= 0x30 && scalarValue <= 0x39) || scalarValue == 0x23 || scalarValue == 0x2A {
            return trimmed.unicodeScalars.contains { $0.value == 0xFE0F }
        }

        return true
    }
}
