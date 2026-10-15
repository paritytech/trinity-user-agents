import Foundation

/// The attributed strings of recent message texts, by text and style.
///
/// The room screen makes the configuration of each message of its window again at each list update, and the
/// Markdown parse and the link detection are the largest part of that work. A message that did not change
/// gives the same text, so the cache gives back the string that the last update made (P7.1, B3).
final class ChatAttributedTextCache: @unchecked Sendable {
    static let shared = ChatAttributedTextCache()

    private final class Entry {
        let value: AttributedString

        init(_ value: AttributedString) {
            self.value = value
        }
    }

    private let cache = NSCache<NSString, Entry>()

    init(countLimit: Int = 2_000) {
        cache.countLimit = countLimit
    }

    /// The string for `text` in `style`, made by `make` when the cache does not hold it.
    func attributedString(for text: String, style: String, make: () -> AttributedString) -> AttributedString {
        let key = "\(style)\u{0}\(text)" as NSString
        if let entry = cache.object(forKey: key) {
            return entry.value
        }

        let value = make()
        cache.setObject(Entry(value), forKey: key)
        return value
    }
}
