import CoreData
import Foundation
@testable import polkadot_app

/// Inserts contacts into an in-memory user database.
struct ChatContactsFixture {
    let facade: UserDataStorageTestFacade

    init(facade: UserDataStorageTestFacade = UserDataStorageTestFacade()) {
        self.facade = facade
    }

    func insertContact(_ peer: Data, isBlocked: Bool = false, hasPendingRequest: Bool = false) async throws {
        try await facade.databaseService.performWrite { context in
            let contact = CDChatContact(context: context)
            contact.identifier = peer.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)
            contact.ownSignKeyId = "//wallet//main"
            contact.isBlocked = isBlocked

            if hasPendingRequest {
                let request = CDChatRequest(context: context)
                request.identifier = UUID().uuidString
                request.status = Chat.RequestStatus.outgoing.rawValue
                request.contact = contact
            }
        }
    }
}
