import Foundation

struct UsernameRequestModel {
    let prefix: String
    let cursor: String?

    init(
        prefix: String,
        caseSensitive: Bool = false,
        cursor: String? = nil
    ) {
        self.prefix = caseSensitive ? prefix : prefix.lowercased()
        self.cursor = cursor
    }
}
