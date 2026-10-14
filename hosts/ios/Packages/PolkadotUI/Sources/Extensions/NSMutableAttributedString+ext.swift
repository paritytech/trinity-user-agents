import Foundation

extension NSMutableAttributedString {
    @objc func applyAttributes(
        _ attributes: [NSAttributedString.Key: Any],
        toOccurrencesOf substring: String,
        options: String.CompareOptions = []
    ) {
        guard !substring.isEmpty, !string.isEmpty else { return }
        let full = string
        var search = full.startIndex ..< full.endIndex

        while let range = full.range(of: substring, options: options, range: search) {
            addAttributes(attributes, range: NSRange(range, in: full))
            search = range.upperBound ..< full.endIndex
        }
    }
}
