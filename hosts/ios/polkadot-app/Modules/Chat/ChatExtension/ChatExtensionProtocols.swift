import Foundation
import UIKitExt

protocol ChatExtensionProcessingContextProtocol {
    func addNewMessage(
        _ content: Chat.LocalMessage.Content,
        delayDelivery: MessageDeliveryDelay,
        chatExtension: ChatExtending
    ) async throws

    func modifyMessageContent(
        messageId: Chat.MessageId,
        content: Chat.LocalMessage.Content
    ) async throws
}

protocol ChatExtending {
    var identifier: ChatExtension.Id { get }

    var customDecoders: [ChatMessageCustomDecoding] { get }

    func activeIn(chat: Chat.Id) -> Bool

    func attach(presentationView: ControllerBackedProtocol)

    func process(
        message: Chat.LocalMessage,
        lastProcessingOutcome: ChatExtension.ProcessingHistoryOutcome,
        context: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult

    func process(action: Chat.Action, context: ChatExtensionActionContextProtocol) async

    func entryRoute(for model: ChatOpenModel) async -> ChatExtensionEntryRoute
}

extension ChatExtending {
    var customDecoders: [ChatMessageCustomDecoding] { [] }

    func entryRoute(for model: ChatOpenModel) async -> ChatExtensionEntryRoute {
        .chat(model)
    }
}

enum ChatExtensionEntryRoute {
    case chat(ChatOpenModel)
    case deepLink(URL)
}
