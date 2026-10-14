import Foundation
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class DeviceUpdateProcessor {
    private let contactsStorageService: ContactsLocalStorageServicing
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let workQueue: DispatchQueue
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        contactsStorageService: ContactsLocalStorageServicing,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.contactsStorageService = contactsStorageService
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.workQueue = workQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

// MARK: - IncomingMessageProcessing

extension DeviceUpdateProcessor: IncomingMessageProcessing {
    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact) {
        guard messageExchangeModeProvider.mode(for: contact) == .multidevice else {
            return
        }

        let changes = extractDeviceChanges(from: messages)

        guard !changes.isEmpty else {
            return
        }

        let settings = Chat.ContactDeviceSettings(
            accountId: contact.accountId,
            changes: changes
        )

        let operation = contactsStorageService.updateDeviceSettings([settings])

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [weak self] result in
            switch result {
            case .success:
                self?.logger.info("Device settings updated for \(contact.username)")
            case let .failure(error):
                self?.logger.warning("Device settings update failed for \(contact.username): \(error)")
            }
        }
    }
}

// MARK: - Private

private extension DeviceUpdateProcessor {
    func extractDeviceChanges(from messages: [Chat.RemoteMessage]) -> [Chat.DeviceChange] {
        let sortedMessages = messages.sorted { $0.timestamp < $1.timestamp }

        return sortedMessages.compactMap { message in
            guard let content = message.versioned.ensureV1()?.content else {
                return nil
            }

            switch content {
            case let .deviceAdded(addedContent):
                let device = Chat.PeerDevice(
                    statementAccountId: addedContent.statementAccountId,
                    encryptionPublicKey: addedContent.encryptionPublicKey
                )
                return .added(device)

            case let .deviceRemoved(removedContent):
                return .removed(statementAccountId: removedContent.statementAccountId)

            default:
                return nil
            }
        }
    }
}
