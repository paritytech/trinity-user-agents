import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct ContactOutgoingSettings: Equatable {
        let accountId: AccountId
        let peerPushId: String?
        let ownPushToken: Data?

        init(accountId: AccountId, peerPushId: String?, ownPushToken: Data?) {
            self.accountId = accountId
            self.peerPushId = peerPushId
            self.ownPushToken = ownPushToken
        }

        init(contact: Chat.Contact) {
            accountId = contact.accountId
            peerPushId = contact.pushId
            ownPushToken = contact.lastOwnToken
        }

        func updatingOwnPushToken(_ ownPushToken: Data?) -> ContactOutgoingSettings {
            ContactOutgoingSettings(
                accountId: accountId,
                peerPushId: peerPushId,
                ownPushToken: ownPushToken
            )
        }

        func updatingPeerPushId(_ peerPushId: String) -> ContactOutgoingSettings {
            ContactOutgoingSettings(
                accountId: accountId,
                peerPushId: peerPushId,
                ownPushToken: ownPushToken
            )
        }
    }
}

final class ContactOutgoingSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.ContactOutgoingSettings
    typealias CoreDataEntity = CDChatContact
}

extension ContactOutgoingSettingsMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingContact
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingContact
        }

        if let pushId = model.peerPushId, pushId != entity.pushId {
            entity.pushId = pushId
        }

        guard let ownPushToken = model.ownPushToken else {
            entity.lastOwnToken = nil
            return
        }

        // update push token if changed and schedule peer message
        guard
            ownPushToken != entity.lastOwnToken
        else {
            return
        }
        entity.lastOwnToken = ownPushToken

        let tokenContent = Chat.RemoteTokenContent(
            token: ownPushToken,
            pushType: .ios
        )

        let localMessage = Chat.LocalMessage.newMessageToPerson(
            model.accountId,
            content: .token(tokenContent)
        )

        let newMessageEntity = try context.insertNew(CDChatMessage.self)

        try ChatMessageEntityMapper().populate(
            entity: newMessageEntity,
            from: localMessage,
            using: context
        )
    }
}

extension Chat.ContactOutgoingSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
