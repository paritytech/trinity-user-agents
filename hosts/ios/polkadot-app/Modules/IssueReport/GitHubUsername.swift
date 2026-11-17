import Foundation

/// GitHub login a reporter gave, posted unescaped as a mention so GitHub notifies them.
struct GitHubUsername: Sendable {
    let value: String

    /// Parses what a reporter typed, ignoring surrounding whitespace and one leading `@`.
    ///
    /// Fails unless the rest is a GitHub login: at most 39 letters, digits and single
    /// hyphens, with no hyphen at either end.
    init?(_ input: String) {
        var name = input.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.hasPrefix("@") {
            name.removeFirst()
        }
        guard name.count <= 39, name.range(of: "^[A-Za-z0-9]+(-[A-Za-z0-9]+)*$", options: .regularExpression) != nil
        else {
            return nil
        }
        value = name
    }
}
