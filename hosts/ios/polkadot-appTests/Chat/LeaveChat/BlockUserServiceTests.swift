import CoreData
import Foundation
import Testing
@testable import polkadot_app

struct BlockUserServiceTests {
    private let fixture = ChatContactsFixture()
    private let cleanupService = SpyAllowanceCleanupService()
    private let peerAccountId = Data(repeating: 0x01, count: 32)

    @Test func cleansUpAllowancesAfterContactIsBlocked() async throws {
        try await fixture.insertContact(peerAccountId)
        var blockedAtCleanup: Bool?
        cleanupService.onCleanup = { [self] in blockedAtCleanup = try? await isContactBlocked() }

        try await makeService().blockUser(accountId: peerAccountId)

        #expect(cleanupService.cleanupCount == 1)
        #expect(blockedAtCleanup == true)
    }

    @Test func unblockingKeepsAllowancesUntouched() async throws {
        try await fixture.insertContact(peerAccountId, isBlocked: true)

        try await makeService().unblockUser(accountId: peerAccountId)

        #expect(cleanupService.cleanupCount == 0)
    }
}

private extension BlockUserServiceTests {
    func makeService() -> BlockUserService {
        BlockUserService(allowanceCleanupService: cleanupService, storageFacade: fixture.facade)
    }

    func isContactBlocked() async throws -> Bool {
        let contacts = try await ContactsLocalStorageService(storageFacade: fixture.facade, logger: MockLogger())
            .getEstablishedContacts()
            .asyncExecute()
        return contacts.first { $0.accountId == peerAccountId }?.isBlocked ?? false
    }
}
