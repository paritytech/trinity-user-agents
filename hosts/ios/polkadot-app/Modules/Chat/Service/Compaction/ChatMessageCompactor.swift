import Foundation
import MessageExchangeKit
import HandoffService
import SubstrateSdk
import KeyDerivation
import StructuredConcurrency
import Individuality

final class ChatMessageCompactor: MessageCompacting, @unchecked Sendable {
    typealias Message = Chat.OpaqueMessage

    private let nodeProvider: HOPNodeProviding
    private let loaderFactory: HOPFileLoaderMaking
    private let proofWallet: WalletManaging
    private let allowanceManager: AllowanceManaging
    private let logger: LoggerProtocol
    private let maxAttempts: Int
    private let retryDelay: Duration

    init(
        nodeProvider: HOPNodeProviding,
        loaderFactory: HOPFileLoaderMaking,
        proofWallet: WalletManaging,
        allowanceManager: AllowanceManaging,
        maxAttempts: Int = 3,
        retryDelay: Duration = .seconds(2),
        logger: LoggerProtocol
    ) {
        self.nodeProvider = nodeProvider
        self.loaderFactory = loaderFactory
        self.proofWallet = proofWallet
        self.allowanceManager = allowanceManager
        self.maxAttempts = maxAttempts
        self.retryDelay = retryDelay
        self.logger = logger
    }

    func compact(messages: [Chat.OpaqueMessage]) async throws -> Chat.OpaqueMessage {
        try await withRetry(maxAttempts: maxAttempts, initialDelay: retryDelay) { [self] in
            do {
                return try await performCompaction(messages: messages)
            } catch {
                logger.warning("Compaction attempt failed: \(error)")
                throw error
            }
        }
    }
}

private extension ChatMessageCompactor {
    func performCompaction(messages: [Chat.OpaqueMessage]) async throws -> Chat.OpaqueMessage {
        // RFC-0002: the batch plaintext is the bare SCALE-encoded [EncodedMessage] list —
        // versioning comes from the pool envelope and each message's own encoding.
        let encodedMessages = try messages.scaleEncoded()

        let node = try nodeProvider.selectNodeOrError()

        let ticket = try FileTicket.generateFileTicket()

        let recipients = try FileRecipients(ticket: ticket)
        let loader = try loaderFactory.makeLoader(for: node)

        let accountId = try proofWallet.getRawPublicKey()
        try await allowanceManager.allocate(accountId: accountId, policy: .ignore, priority: .normal)

        let sender = try SenderProofProvider(sender: proofWallet.getMultiSigner()) { [proofWallet] data in
            try proofWallet.sign(data: data)
        }

        let hash = try await loader.uploadBlob(
            encodedMessages,
            sender: sender,
            recipients: recipients
        )

        logger.debug("Compacted \(messages.count) messages, hash: \(hash.toHex())")

        let remoteContent = ChatRemoteMessageContent.CompactedMessagesContent(
            claimIdentifier: hash,
            claimTicket: ticket,
            node: node
        )

        let remoteMessage = Chat.RemoteMessage(
            messageId: UUID().uuidString,
            timestamp: Date().toChatTimestamp(),
            versioned: .v1(.init(content: .compactedMessages(remoteContent)))
        )

        logger.debug("Compaction completed for \(messages.count) messages")

        return Chat.OpaqueMessage(remoteMessage: remoteMessage)
    }
}
