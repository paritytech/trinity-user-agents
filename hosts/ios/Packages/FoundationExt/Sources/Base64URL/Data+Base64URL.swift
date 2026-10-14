import Foundation

public extension Data {
    /// Base64URL (RFC 4648 §5): `-`/`_` replace `+`/`/`, `=` padding stripped.
    func base64URLEncodedString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    /// Strict Base64URL decode — rejects `+`/`/` (even though `Data(base64Encoded:)`
    /// would accept them) so a config typo using the standard alphabet fails closed.
    init?(base64URLEncoded string: String) {
        var standard = String()
        standard.reserveCapacity(string.count + 4)
        for character in string {
            switch character {
            case "+",
                 "/": return nil
            case "-": standard.append("+")
            case "_": standard.append("/")
            default: standard.append(character)
            }
        }
        let padCount = (4 - standard.count % 4) % 4
        standard.append(String(repeating: "=", count: padCount))
        self.init(base64Encoded: standard)
    }
}
