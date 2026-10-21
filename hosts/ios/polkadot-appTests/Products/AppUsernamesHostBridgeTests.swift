import Foundation
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product finds people by username. The product gets usernames only.
struct AppUsernamesHostBridgeTests {
    @Test func aSearchGivesEachUsernameOnceAndAtMostTwenty() async throws {
        let people = try (0 ..< 30).map { try contact("alice.\($0)", account: UInt8($0)) } + [contact(
            "Alice.0",
            account: 99
        )]
        let bridge = makeBridge(directory: people)

        let usernames = try await bridge.searchUsernames(productId: "verbosity.dot", prefix: "al")

        #expect(usernames == (0 ..< 20).map { "alice.\($0)" })
    }
}

// MARK: - Doubles

private func accountId(_ byte: UInt8) -> AccountId {
    Data(repeating: byte, count: 32)
}

private func contact(_ username: String, account: UInt8) throws -> Chat.RemoteContact {
    try Chat.RemoteContact(
        accountId: accountId(account),
        username: username,
        chatPublicKey: Chat.PublicKey(rawData: Data(repeating: account, count: 32)),
        imageData: nil
    )
}

private func makeBridge(directory: [Chat.RemoteContact]) -> AppUsernamesHostBridge {
    AppUsernamesHostBridge(directory: FixedDirectory(people: directory))
}

private final class FixedDirectory: RemoteContactOperationMaking, @unchecked Sendable {
    private let people: [Chat.RemoteContact]

    init(people: [Chat.RemoteContact]) {
        self.people = people
    }

    func search(by query: String) async throws -> [Chat.RemoteContact] {
        people.filter { $0.username.lowercased().hasPrefix(query.lowercased()) }
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        people.first { $0.accountId == accountId }
    }
}

