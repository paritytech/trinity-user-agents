import Foundation
import os
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product finds people by username and asks the host to add one of them. The product gets usernames only, and
/// the host asks the user before it opens the chat request screen of the app.
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

    @Test func anAddThatTheUserAcceptsOpensTheRequestScreen() async throws {
        let screen = RecordingScreen(answer: true)
        let bridge = try makeBridge(directory: [contact("bob.7", account: 2)], screen: screen)

        let outcome = try await bridge.addContact(
            productId: "verbosity.dot",
            executionKind: .worker,
            username: "@Bob.7"
        )

        #expect(outcome == .added)
        #expect(screen.questions == ["verbosity.dot asks about bob.7"])
        #expect(screen.requests == [accountId(2)])
    }

    @Test func anAddEndsWithoutAQuestionForAnUnknownUsernameOrAContact() async throws {
        let screen = RecordingScreen(answer: true)
        let bridge = try makeBridge(directory: [contact("bob.7", account: 2)], contacts: [accountId(2)], screen: screen)

        let unknown = try await bridge.addContact(productId: "verbosity.dot", executionKind: .worker, username: "bob")
        let known = try await bridge.addContact(productId: "verbosity.dot", executionKind: .worker, username: "bob.7")

        #expect([unknown, known] == [HostContactAdd.notFound, .alreadyContact])
        #expect(screen.questions.isEmpty)
    }

    @Test func anAddThatTheUserRefusesOrThatNoScreenShowsOpensNothing() async throws {
        let refusing = RecordingScreen(answer: false)
        let hidden = RecordingScreen(answer: nil)
        let people = try [contact("bob.7", account: 2)]

        let refused = try await makeBridge(directory: people, screen: refusing)
            .addContact(productId: "verbosity.dot", executionKind: .worker, username: "bob.7")
        let unseen = try await makeBridge(directory: people, screen: hidden)
            .addContact(productId: "verbosity.dot", executionKind: .app, username: "bob.7")

        #expect([refused, unseen] == [HostContactAdd.dismissed, .notVisible])
        #expect(refusing.requests.isEmpty && hidden.requests.isEmpty)
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

private func makeBridge(
    directory: [Chat.RemoteContact],
    contacts: Set<AccountId> = [],
    screen: RecordingScreen = RecordingScreen(answer: true)
) -> AppUsernamesHostBridge {
    AppUsernamesHostBridge(
        directory: FixedDirectory(people: directory),
        contactAccounts: { contacts },
        screen: screen
    )
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

private final class RecordingScreen: ProductAddContactScreen, @unchecked Sendable {
    private let answer: Bool?
    private let lock = OSAllocatedUnfairLock(initialState: (questions: [String](), requests: [AccountId]()))

    init(answer: Bool?) {
        self.answer = answer
    }

    var questions: [String] { lock.withLock { $0.questions } }
    var requests: [AccountId] { lock.withLock { $0.requests } }

    func confirmAdd(product: String, username: String) async -> Bool? {
        lock.withLock { $0.questions.append("\(product) asks about \(username)") }
        return answer
    }

    func openRequest(to contact: Chat.RemoteContact) async throws {
        lock.withLock { $0.requests.append(contact.accountId) }
    }
}
