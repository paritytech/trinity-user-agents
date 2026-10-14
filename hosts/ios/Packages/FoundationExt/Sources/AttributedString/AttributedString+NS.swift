import Foundation

public extension NSAttributedString {
    func toAttributedStringOrEmpty() -> AttributedString {
        (try? AttributedString(self, including: \.uiKit)) ?? AttributedString()
    }
}
