import Foundation
import HandoffService
import SubstrateSdk

typealias MessagesExpansionClosure = ([Chat.RemoteMessage]) async throws -> Void

protocol ChatMessageClaiming {
    func claim(
        message: Chat.LocalMessage,
        onExpansion: @escaping MessagesExpansionClosure
    ) async throws
}

final class ChatMessageClaimer {
    private let nodeProvider: HOPNodeProviding
    private let loaderFactory: HOPFileLoaderMaking
    private let logger: LoggerProtocol

    init(
        nodeProvider: HOPNodeProviding,
        loaderFactory: HOPFileLoaderMaking,
        logger: LoggerProtocol
    ) {
        self.nodeProvider = nodeProvider
        self.loaderFactory = loaderFactory
        self.logger = logger
    }
}

extension ChatMessageClaimer: ChatMessageClaiming {
    func claim(
        message: Chat.LocalMessage,
        onExpansion: @escaping MessagesExpansionClosure
    ) async throws {
        guard case let .compactedMessages(content) = message.content else {
            throw ClaimError.notCompactedMessage
        }

        guard nodeProvider.isNodeAllowed(content.node) else {
            throw ClaimError.untrustedNode
        }

        let claimer = try FileClaimer(ticket: content.claimTicket)
        let loader = try loaderFactory.makeLoader(for: content.node)

        logger.debug("Messages claiming for \(message.messageId) starting...")

        try await loader.downloadBlob(
            content.claimIdentifier,
            claimer: claimer
        ) { data in
            let batch = try [Chat.OpaqueMessage].fromScaleEncoded(data)
            let messages = batch.map(\.remoteMessage)
            try await onExpansion(messages)
        }

        logger.debug("Messages claiming for \(message.messageId) completed")
    }
}

extension ChatMessageClaimer {
    enum ClaimError: Error {
        case notCompactedMessage
        case untrustedNode
    }
}
