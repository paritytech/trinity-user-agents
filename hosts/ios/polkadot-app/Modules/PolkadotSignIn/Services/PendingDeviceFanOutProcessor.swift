import Foundation
import AsyncExtensions
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

actor PendingDeviceFanOutProcessor {
    private let chatContactDataProviderFactory: ChatContactDataProviderMaking
    private let fanOutSettingsRepository: AnyDataProviderRepository<Chat.ContactDevicesFanOutSettings>
    private let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?
    private var inFlightContactIds = Set<AccountId>()

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        messageRepositoryFactory: ChatMessageRepositoryMaking = ChatMessageRepositoryFactory(),
        localDeviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.messageExchangeModeProvider = messageExchangeModeProvider
        fanOutSettingsRepository = AnyDataProviderRepository(storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ContactDevicesFanOutSettingsMapper())
        ))
        messageRepository = messageRepositoryFactory.createRepository(forFilter: nil)
        localDeviceRepository = localDeviceRepositoryFactory.createRepository(forFilter: nil)
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }

    func setup() {
        logger.debug("Pending device fan-out processor setup")
        throttle()

        subscriptionTask = Task { [weak self] in
            await self?.runSubscription()
        }
    }

    func throttle() {
        subscriptionTask?.cancel()
        subscriptionTask = nil
        inFlightContactIds = []
    }
}

private extension PendingDeviceFanOutProcessor {
    func runSubscription() async {
        do {
            let sequence = chatContactDataProviderFactory
                .subscribeContactsWithPredicate(.pendingDevicesFanOutContacts())

            for try await contacts in sequence {
                try Task.checkCancellation()
                logger.debug("Update: \(contacts.count) contacts; \(inFlightContactIds.count) inFlight")
                try await handlePendingContacts(contacts)
            }
        } catch is CancellationError {
            return
        } catch {
            logger.error("Subscription failed: \(error)")
        }
    }

    func handlePendingContacts(_ contacts: [Chat.Contact]) async throws {
        do {
            try await broadcastPendingLocalDevices(for: contacts)
        } catch let error as CancellationError {
            throw error
        } catch {
            logger.error("Broadcasting failed: \(error)")
        }
    }

    func broadcastPendingLocalDevices(for contacts: [Chat.Contact]) async throws {
        let contactsToProcess = beginFanOut(for: contacts.filter {
            $0.pendingDevicesFanOut &&
                $0.chatRequest == nil &&
                !$0.isBlocked &&
                messageExchangeModeProvider.mode(for: $0) == .multidevice
        })

        guard !contactsToProcess.isEmpty else {
            logger.debug("No eligible contacts to process")
            return
        }

        logger.debug("Processing \(contactsToProcess.count) contacts")

        defer { finishFanOut(for: contactsToProcess) }

        let localDevices = try await localDeviceRepository
            .fetchAllOperation(with: .init())
            .asyncExecute()

        guard !localDevices.isEmpty else {
            logger.debug("No local devices; clearing pending flags")
            try await clearPendingDevicesFanOut(for: contactsToProcess)
            return
        }

        logger.debug("Local devices count: \(localDevices.count)")

        let messages = contactsToProcess.flatMap { contact in
            localDevices.map { device in
                Chat.LocalMessage.newMessageToPerson(
                    contact.accountId,
                    content: .deviceAdded(.init(
                        statementAccountId: device.statementAccountId,
                        encryptionPublicKey: device.encryptionPublicKey
                    ))
                )
            }
        }

        logger.debug("Saving \(messages.count) deviceAdded messages")

        try await saveMessages(messages)
        try await clearPendingDevicesFanOut(for: contactsToProcess)
    }

    func saveMessages(_ messages: [Chat.LocalMessage]) async throws {
        guard !messages.isEmpty else {
            return
        }

        try await messageRepository
            .saveOperation({ messages }, { [] })
            .asyncExecute()

        logger.debug("Saved \(messages.count) deviceAdded messages")
    }

    func clearPendingDevicesFanOut(for contacts: [Chat.Contact]) async throws {
        let settings = contacts.map {
            Chat.ContactDevicesFanOutSettings(
                accountId: $0.accountId,
                pendingDevicesFanOut: false
            )
        }

        try await fanOutSettingsRepository
            .saveOperation({ settings }, { [] })
            .asyncExecute()

        logger.debug("Cleared flags for \(contacts.count) contacts")
    }

    func beginFanOut(for contacts: [Chat.Contact]) -> [Chat.Contact] {
        var result = [Chat.Contact]()

        for contact in contacts where !inFlightContactIds.contains(contact.accountId) {
            inFlightContactIds.insert(contact.accountId)
            result.append(contact)
        }

        return result
    }

    func finishFanOut(for contacts: [Chat.Contact]) {
        contacts.forEach { inFlightContactIds.remove($0.accountId) }
    }
}
