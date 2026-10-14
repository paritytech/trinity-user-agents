import Foundation

public extension NSAttributedString {
    func uppercased() -> NSAttributedString {
        let result = NSMutableAttributedString(attributedString: self)

        result.enumerateAttributes(in: NSRange(location: 0, length: length), options: []) { _, range, _ in
            result.replaceCharacters(in: range, with: (string as NSString).substring(with: range).uppercased())
        }

        return result
    }
}
