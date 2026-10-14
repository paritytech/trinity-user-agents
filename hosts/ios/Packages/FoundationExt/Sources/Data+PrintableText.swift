import Foundation

public extension Data {
    /// UTF-8 decoded string when the bytes form printable text
    /// (whitespace and newlines allowed); nil otherwise.
    var printableUTF8String: String? {
        guard !isEmpty, let string = String(data: self, encoding: .utf8) else {
            return nil
        }

        let allowedControls = CharacterSet.whitespacesAndNewlines
        let hasUnprintable = string.unicodeScalars.contains { scalar in
            CharacterSet.controlCharacters.contains(scalar) && !allowedControls.contains(scalar)
        }

        return hasUnprintable ? nil : string
    }
}
