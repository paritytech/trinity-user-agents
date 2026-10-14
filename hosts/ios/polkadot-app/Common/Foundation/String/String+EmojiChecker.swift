import PolkadotUI

public extension String {
    var isSingleEmoji: Bool {
        SingleEmojiChecker.isSingleEmoji(self)
    }
}
