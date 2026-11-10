import Foundation
import CoreData
import SubstrateSdk
import MessageExchangeKit

extension NSManagedObjectContext {
    @discardableResult
    func setupIncomingRequestMessage(model: ChatRequest.ValidatedRemoteModel) throws -> CDChatMessage {
        let localMessage = Chat.LocalMessage(
            chatRequest: model.message,
            creationSource: .localDevice,
            status: .incoming(.new),
            contactId: model.peerAccountId
        )

        let optMessage: CDChatMessage? = try first(for: .chatMessage(with: model.message.messageId))

        guard optMessage == nil else {
            throw CoreDataMapperError.unexpected("Existing message entity")
        }

        let message = CDChatMessage(context: self)

        try ChatMessageEntityMapper().populate(
            entity: message,
            from: localMessage,
            using: self
        )

        return message
    }

    @discardableResult
    func setupOutgoingRequestMessage(
        _ message: Chat.RequestMessage,
        peerAccountId: AccountId
    ) throws -> CDChatMessage {
        let localMessage = Chat.LocalMessage(
            chatRequest: message,
            creationSource: .localDevice,
            status: .outgoing(.new),
            contactId: peerAccountId
        )

        let optMessage: CDChatMessage? = try first(for: .chatMessage(with: message.messageId))

        guard optMessage == nil else {
            throw CoreDataMapperError.unexpected("Existing message entity")
        }

        let message = CDChatMessage(context: self)

        try ChatMessageEntityMapper().populate(
            entity: message,
            from: localMessage,
            using: self
        )

        return message
    }

    /// The notification extension may have saved it already, and a missing chat must not fail the accept.
    func setupPeerAcceptanceMessage(_ acceptance: Chat.RemoteMessage, from contact: CDChatContact) throws {
        let existing: CDChatMessage? = try first(for: .chatMessage(with: acceptance.messageId))

        guard
            existing == nil,
            contact.chat != nil,
            let peerAccountId = try contact.identifier?.fromHex(),
            let localMessage = Chat.LocalMessage(
                remote: acceptance,
                creationSource: .localDevice,
                status: .incoming(.new),
                contactId: peerAccountId
            )
        else {
            return
        }

        try ChatMessageEntityMapper().populate(
            entity: CDChatMessage(context: self),
            from: localMessage,
            using: self
        )
    }

    @discardableResult
    func setupAcceptMessage(
        for requestId: String,
        peerAccountId: AccountId,
        acceptorDevice: Chat.PeerDevice
    ) throws -> CDChatMessage {
        let acceptMessageEntity = CDChatMessage(context: self)

        let acceptMessageModel = Chat.LocalMessage.newMessageToPerson(
            peerAccountId,
            content: .multiChatAccepted(.init(requestId: requestId, device: acceptorDevice))
        )

        try ChatMessageEntityMapper().populate(
            entity: acceptMessageEntity,
            from: acceptMessageModel,
            using: self
        )

        return acceptMessageEntity
    }
}
