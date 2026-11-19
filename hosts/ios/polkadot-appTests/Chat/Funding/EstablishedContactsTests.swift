import Foundation
import Testing
@testable import polkadot_app

struct EstablishedContactsTests {
    private let fixture = ChatContactsFixture()
    private let establishedPeer = Data(repeating: 0x01, count: 32)
    private let blockedPeer = Data(repeating: 0x02, count: 32)
    private let pendingPeer = Data(repeating: 0x03, count: 32)

    @Test func includeBlockedAndExcludePendingRequests() async throws {
        try await fixture.insertContact(establishedPeer)
        try await fixture.insertContact(blockedPeer, isBlocked: true)
        try await fixture.insertContact(pendingPeer, hasPendingRequest: true)

        let contacts = try await ContactsLocalStorageService(storageFacade: fixture.facade, logger: MockLogger())
            .getEstablishedContacts()
            .asyncExecute()

        #expect(Set(contacts.map(\.accountId)) == [establishedPeer, blockedPeer])
    }
}
