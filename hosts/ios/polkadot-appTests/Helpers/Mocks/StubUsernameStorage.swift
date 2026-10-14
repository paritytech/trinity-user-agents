import Foundation
@testable import polkadot_app

/// Keeps the username in memory.
final class StubUsernameStorage: UsernameStoring {
    var username: Username?
    var usernameClaimed = false

    init(username: String? = "tester.01") {
        self.username = username.map { Username(value: $0) }
    }
}
