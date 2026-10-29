import Foundation
import Individuality
import Testing
@testable import polkadot_app

struct ChatAllowanceFilterTests {
    private let fixture = ChatContactsFixture()
    private let establishedPeer = Data(repeating: 0x0A, count: 32)
    private let pendingPeer = Data(repeating: 0x0B, count: 32)
    private let blockedPeer = Data(repeating: 0x0E, count: 32)
    private let signerManager = ChatSignerManager(
        entropyManager: FixedRootEntropyManager(),
        tldProvider: StubDotNsTldProvider(tld: "dot")
    )

    @Test func releasesChatRecordsWithoutEstablishedChat() async throws {
        let established = try record(for: signerManager.makeSigner(with: establishedPeer).accountId, domain: .chat)
        let pending = try record(for: signerManager.makeSigner(with: pendingPeer).accountId, domain: .chat)
        let orphan = record(for: Data(repeating: 0x0C, count: 32), domain: .chat)
        try await fixture.insertContact(establishedPeer)
        try await fixture.insertContact(pendingPeer, hasPendingRequest: true)

        let released = try await makeFilter().recordsToRelease(from: [established, pending, orphan])

        #expect(released.map(\.accountId) == [pending.accountId, orphan.accountId])
    }

    @Test func releasesChatRecordsOfBlockedContacts() async throws {
        let established = try record(for: signerManager.makeSigner(with: establishedPeer).accountId, domain: .chat)
        let blocked = try record(for: signerManager.makeSigner(with: blockedPeer).accountId, domain: .chat)
        try await fixture.insertContact(establishedPeer)
        try await fixture.insertContact(blockedPeer, isBlocked: true)

        let released = try await makeFilter().recordsToRelease(from: [established, blocked])

        #expect(released.map(\.accountId) == [blocked.accountId])
    }

    @Test func neverReleasesOtherDomains() async throws {
        let records = [AllowanceRecord.Domain.device, .product, .coinage].map { domain in
            record(for: Data(repeating: 0x0D, count: 32), domain: domain)
        }

        let released = try await makeFilter().recordsToRelease(from: records)

        #expect(released.isEmpty)
    }
}

private extension ChatAllowanceFilterTests {
    func makeFilter() -> ChatAllowanceFilter {
        ChatAllowanceFilter(
            contactsStorageService: ContactsLocalStorageService(storageFacade: fixture.facade, logger: MockLogger()),
            signerManager: signerManager
        )
    }

    func record(for accountId: Data, domain: AllowanceRecord.Domain) -> AllowanceRecord {
        AllowanceRecord(
            accountId: accountId,
            allocatedAt: Date(timeIntervalSince1970: 1_000),
            kind: .statementStore,
            domain: domain,
            priority: .critical,
            latestRenewedPeriod: 1
        )
    }
}
