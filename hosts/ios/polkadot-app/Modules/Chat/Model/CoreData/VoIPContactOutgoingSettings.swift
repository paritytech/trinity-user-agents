import Foundation
import Operation_iOS
import Foundation_iOS
import CoreData
import SubstrateSdk

extension Chat {
    struct VoIPContactOutgoingSettings: Equatable {
        let accountId: AccountId
        let peerPushId: String?
        let voipOwnPushToken: Data?

        init(accountId: AccountId, peerPushId: String?, voipOwnPushToken: Data?) {
            self.accountId = accountId
            self.peerPushId = peerPushId
            self.voipOwnPushToken = voipOwnPushToken
        }

        init(contact: Chat.Contact) {
            accountId = contact.accountId
            peerPushId = contact.pushId
            voipOwnPushToken = contact.voipLastOwnToken
        }

        func updatingVoIPOwnPushToken(_ voipOwnPushToken: Data?) -> VoIPContactOutgoingSettings {
            VoIPContactOutgoingSettings(
                accountId: accountId,
                peerPushId: peerPushId,
                voipOwnPushToken: voipOwnPushToken
            )
        }

        func updatingPeerPushId(_ peerPushId: String) -> VoIPContactOutgoingSettings {
            VoIPContactOutgoingSettings(
                accountId: accountId,
                peerPushId: peerPushId,
                voipOwnPushToken: voipOwnPushToken
            )
        }
    }
}

final class VoIPContactOutgoingSettingsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.VoIPContactOutgoingSettings
    typealias CoreDataEntity = CDChatContact
}

extension VoIPContactOutgoingSettingsMapper: CoreDataMapperProtocol {
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

        guard let voipOwnPushToken = model.voipOwnPushToken else {
            entity.voipLastOwnToken = nil
            return
        }

        // update push token if changed and schedule peer message
        guard
            voipOwnPushToken != entity.voipLastOwnToken
        else {
            return
        }
        entity.voipLastOwnToken = voipOwnPushToken

        let tokenContent = Chat.RemoteTokenContent(
            token: voipOwnPushToken,
            pushType: .iosVoIP
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

extension Chat.VoIPContactOutgoingSettings: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
