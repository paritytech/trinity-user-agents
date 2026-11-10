import Foundation

struct UsernameRequestModel {
    let prefix: String

    init(
        prefix: String,
        caseSensitive: Bool = false
    ) {
        self.prefix = caseSensitive ? prefix : prefix.lowercased()
    }
}
