import Foundation
import Operation_iOS
import StatementDelivery

/// The acceptance is published outside the outbox, so it also sends the push the outbox would have sent.
/// A failed push never fails the delivery.
final class ChatAcceptanceNotifier: @unchecked Sendable {
    private let apnsClientService: APNSClientServicing
    private let notifiedMessageIdRepository: AnyDataProviderRepository<Chat.NotifiedMessageId>
    private let logger: LoggerProtocol

    init(
        apnsClientService: APNSClientServicing,
        notifiedMessageIdRepository: AnyDataProviderRepository<Chat.NotifiedMessageId>,
        logger: LoggerProtocol
    ) {
        self.apnsClientService = apnsClientService
        self.notifiedMessageIdRepository = notifiedMessageIdRepository
        self.logger = logger
    }
}

extension ChatAcceptanceNotifier: OutgoingStatementPostDelivering {
    func didPublishFirstTime(_ input: OutgoingChatAcceptanceInput) async {
        let messageId = input.message.messageId

        guard let remoteMessage = input.message.toRemote() else { return }

        do {
            let alreadyNotified = try await notifiedMessageIdRepository
                .fetchOperation(by: { messageId }, options: RepositoryFetchOptions())
                .asyncExecute() != nil

            guard !alreadyNotified else { return }

            _ = try await apnsClientService
                .notifyAboutNewMessageWrapper(remoteMessage, contact: input.contact)
                .asyncExecute()

            try await notifiedMessageIdRepository
                .saveOperation({ [Chat.NotifiedMessageId(messageId: messageId)] }, { [] })
                .asyncExecute()
        } catch {
            logger.error("Chat acceptance \(messageId) push failed: \(error)")
        }
    }
}
