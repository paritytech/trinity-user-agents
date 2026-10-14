import DesignSystem
import SwiftUI

public struct ChatMessageSingleEmojiView: View, Hashable {
    public static let reuseIdentifier = "ChatMessageSingleEmojiView"

    public let emoji: String

    public init(emoji: String) {
        self.emoji = emoji
    }

    public var body: some View {
        let font = emojiFont
        Text(emoji)
            .font(Font(font))
            .frame(height: font.pointSize)
    }

    // Workaround emojiLarge didn't sizes like in Figma (actual box is bigger)
    // TODO: Update emoji specific fonts in DS
    private var emojiFont: UIFont {
        let font = UIFont.app(.emojiLarge)
        return UIFont(name: "AppleColorEmoji", size: font.pointSize) ?? font
    }
}

// MARK: - Previews

#if DEBUG
    #Preview {
        VStack(spacing: 20) {
            ChatMessageSingleEmojiView(emoji: "🎂")
            ChatMessageSingleEmojiView(emoji: "🐸")
            ChatMessageSingleEmojiView(emoji: "👍🏻")
        }
        .padding()
        .background(Color.black)
    }
#endif
