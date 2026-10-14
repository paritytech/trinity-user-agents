import Foundation
import Operation_iOS
import UIKitExt

final class ChatReactionExtension: ChatExtending {
    let identifier = "reactions"

    private let reactionRepository: ChatReactionRepositoryProtocol
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        reactionRepository: ChatReactionRepositoryProtocol,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.reactionRepository = reactionRepository
        self.operationQueue = operationQueue
        self.logger = logger
    }

    func activeIn(chat _: Chat.Id) -> Bool {
        true
    }

    func process(
        message: Chat.LocalMessage,
        lastProcessingOutcome: ChatExtension.ProcessingHistoryOutcome,
        context _: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        guard lastProcessingOutcome == .firstEncounter else {
            return .skipped
        }

        guard case .contact = message.origin else {
            return .skipped
        }

        switch message.content {
        case let .reacted(reactionContent):
            return await handleReactionAdded(
                reactionContent: reactionContent,
                message: message
            )

        case let .reactionRemoved(reactionContent):
            return await handleReactionRemoved(
                reactionContent: reactionContent,
                message: message
            )

        default:
            return .skipped
        }
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {}

    func attach(presentationView _: ControllerBackedProtocol) {
        // no use for presetnation view
    }
}

// MARK: - Private functions

extension ChatReactionExtension {
    private func handleReactionAdded(
        reactionContent: Chat.RemoteMessageContentV1.MessageContent.ReactionContent,
        message: Chat.LocalMessage
    ) async -> ChatExtension.ProcessingResult {
        let reaction = Chat.MessageReaction(
            messageId: reactionContent.messageId,
            emoji: reactionContent.emoji,
            origin: message.origin,
            chatId: message.chatId,
            timestamp: message.timestamp
        )

        do {
            try await reactionRepository.saveReaction(reaction).asyncExecute()
            return .processed
        } catch {
            return .skipped
        }
    }

    private func handleReactionRemoved(
        reactionContent: Chat.RemoteMessageContentV1.MessageContent.ReactionContent,
        message: Chat.LocalMessage
    ) async -> ChatExtension.ProcessingResult {
        do {
            try await reactionRepository.removeReaction(
                messageId: reactionContent.messageId,
                emoji: reactionContent.emoji,
                origin: message.origin
            )
            .asyncExecute()
            return .processed
        } catch {
            return .processed
        }
    }
}
