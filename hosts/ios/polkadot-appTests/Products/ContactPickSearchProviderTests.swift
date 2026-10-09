import Testing
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
@testable import polkadot_app

/// The product picker searches the user's own contacts. The chat search also
/// reaches the People chain; these pin that the picker does not, because a
/// product is choosing among people the user already keeps.
struct ContactPickSearchProviderTests {
    @Test("Opening the picker lists every stored contact")
    func emptyQueryListsEveryContact() async throws {
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [
            makeContact(accountId: Data(repeating: 0xA1, count: 32), username: "alice"),
            makeContact(accountId: Data(repeating: 0xB0, count: 32), username: "bob")
        ]

        let sections = try await makeProvider(localSearch).search(query: nil)

        #expect(localSearch.didRequestAllContacts)
        #expect(sections.contacts.map(\.username?.value) == ["alice", "bob"])
    }

    @Test("Typing filters the contacts rather than searching dotNS")
    func queryFiltersStoredContacts() async throws {
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [makeContact(username: "alice")]

        let sections = try await makeProvider(localSearch).search(query: "ali")

        #expect(localSearch.receivedUsernamePrefix == "ali")
        #expect(sections.contacts.count == 1)
    }

    /// A product sees the people the user keeps, never strangers a username
    /// lookup would surface.
    @Test("No global section, with or without a query", arguments: [nil, "ali"])
    func noGlobalSection(query: String?) async throws {
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [makeContact(username: "alice")]

        let sections = try await makeProvider(localSearch).search(query: query)

        #expect(sections.global.rows.isEmpty)
        #expect(!sections.global.isPending)
        #expect(sections.recent.isEmpty)
    }

    /// Offering somebody the user refused would be worse than offering nobody,
    /// and the picker has no other section for them to fall into.
    @Test("A blocked contact is never offered")
    func blockedContactIsExcluded() async throws {
        let blocked = Data(repeating: 0xCC, count: 32)
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [
            makeContact(accountId: blocked, username: "mallory", isBlocked: true),
            makeContact(accountId: Data(repeating: 0xA1, count: 32), username: "alice")
        ]

        // Pins the fixture's own precondition, so a seeding failure reads as
        // one rather than as the provider offering a blocked contact.
        let seededRepository = localSearch.blockedContacts()
        let seededBlocked = try await seededRepository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        #expect(seededBlocked.map(\.username) == ["mallory"])

        let sections = try await makeProvider(localSearch).search(query: nil)

        #expect(localSearch.didRequestBlockedContacts)
        #expect(sections.contacts.map(\.username?.value) == ["alice"])
    }

    /// Handing a product a handle for the user themselves would name the user
    /// to the product as if they were somebody else.
    @Test("The user is not among their own contacts")
    func ownAccountIsExcluded() async throws {
        let own = Data(repeating: 0x0E, count: 32)
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [
            makeContact(accountId: own, username: "me"),
            makeContact(accountId: Data(repeating: 0xA1, count: 32), username: "alice")
        ]

        let sections = try await makeProvider(localSearch, ownAccountId: own).search(query: nil)

        #expect(sections.contacts.map(\.username?.value) == ["alice"])
    }

    /// The interactor reads searches as a stream of phases. Everything here is
    /// local, so the stream carries one result and then ends.
    @Test("Searching yields one phase and finishes")
    func searchPhasesYieldsOnce() async throws {
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [makeContact(username: "alice")]

        var phases: [AccountSearchSections<ContactSearchPayload, ContactSearchPayload>] = []
        for try await phase in makeProvider(localSearch).searchPhases(query: "ali") {
            phases.append(phase)
        }

        #expect(phases.count == 1)
        #expect(phases.first?.contacts.map(\.username?.value) == ["alice"])
    }
}

private extension ContactPickSearchProviderTests {
    func makeProvider(
        _ localSearch: MockLocalContactSearch,
        ownAccountId: AccountId = Data(repeating: 0x0E, count: 32)
    ) -> ContactPickSearchProvider {
        ContactPickSearchProvider(
            localContactSearch: localSearch,
            ownAccountId: ownAccountId
        )
    }
}

private func makeContact(
    accountId: AccountId = Data(repeating: 0x01, count: 32),
    username: String = "test_user",
    isBlocked: Bool = false
) -> Chat.Contact {
    Chat.Contact(
        accountId: accountId,
        username: username,
        publicKey: Data(repeating: 0, count: 32),
        pin: nil,
        pushId: nil,
        pushToken: nil,
        voipPushToken: nil,
        peerPlatform: nil,
        lastOwnToken: nil,
        voipLastOwnToken: nil,
        chatRequest: nil,
        ownKeyId: Chat.Contact.Own(signKeyId: "", encryptionKeyId: ""),
        imageData: nil,
        source: .chat,
        isBlocked: isBlocked,
        devices: [],
        pendingDevicesFanOut: false,
        addedAt: nil,
        acceptedAt: nil
    )
}
