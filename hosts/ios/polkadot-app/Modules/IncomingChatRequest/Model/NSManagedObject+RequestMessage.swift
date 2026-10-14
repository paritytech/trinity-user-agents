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

    @discardableResult
    func setupAcceptMessage(
        for requestId: String,
        peerAccountId: AccountId,
        messageExchangeMode: MessageExchangeMode,
        acceptorDevice: Chat.PeerDevice?
    ) throws -> CDChatMessage {
        let acceptMessageEntity = CDChatMessage(context: self)

        let content: Chat.LocalMessage.Content

        switch messageExchangeMode {
        case .identity:
            let acceptContent = ChatRemoteMessageContent.ChatAccepted(
                messageId: requestId
            )
            content = .chatAccepted(acceptContent)
        case .multidevice:
            guard let acceptorDevice else {
                throw CoreDataMapperError.unexpected("acceptorDevice is required for DeviceChatAccepted")
            }
            let acceptContent = ChatRemoteMessageContent.DeviceChatAccepted(
                requestId: requestId,
                device: acceptorDevice
            )
            content = .multiChatAccepted(acceptContent)
        }

        let acceptMessageModel = Chat.LocalMessage.newMessageToPerson(
            peerAccountId,
            content: content
        )

        try ChatMessageEntityMapper().populate(
            entity: acceptMessageEntity,
            from: acceptMessageModel,
            using: self
        )

        return acceptMessageEntity
    }
}
