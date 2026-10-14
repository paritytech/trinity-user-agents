import Foundation
import MessageExchangeKit
import Operation_iOS

struct DeviceSyncCollectedChanges {
    var entities: [Chat.DeviceSyncEntity]
    var timePoint: UInt64
}

struct DeviceSyncOutgoingChangesCollector {
    private let deviceRepositoryFactory: LocalDeviceRepositoryMaking
    private let contactRepositoryFactory: ChatContactRepositoryMaking
    private let messageRepositoryFactory: ChatMessageRepositoryMaking
    private let removedChatRepositoryFactory: RemovedChatRepositoryMaking
    private let messageExchangeModeProvider: any MessageExchangeModeProviding

    init(
        deviceRepositoryFactory: LocalDeviceRepositoryMaking,
        contactRepositoryFactory: ChatContactRepositoryMaking,
        messageRepositoryFactory: ChatMessageRepositoryMaking,
        removedChatRepositoryFactory: RemovedChatRepositoryMaking,
        messageExchangeModeProvider: any MessageExchangeModeProviding
    ) {
        self.deviceRepositoryFactory = deviceRepositoryFactory
        self.contactRepositoryFactory = contactRepositoryFactory
        self.messageRepositoryFactory = messageRepositoryFactory
        self.removedChatRepositoryFactory = removedChatRepositoryFactory
        self.messageExchangeModeProvider = messageExchangeModeProvider
    }

    func collectEntities(
        since checkpoint: UInt64?
    ) async throws -> DeviceSyncCollectedChanges {
        let timePoint = Date().toChatTimestamp()
        let cutoff = checkpoint.map { Date.fromChatTimestamp($0) }
        var entities: [Chat.DeviceSyncEntity] = []

        let deviceChanges = try await collectDeviceChanges(after: cutoff)
        let contactChanges = try await collectContactChanges(after: cutoff)
        let removedChatChanges = try await collectRemovedChatChanges(after: cutoff)
        let messageChanges = try await collectMessageChanges(since: checkpoint)

        entities.append(contentsOf: deviceChanges)
        entities.append(contentsOf: contactChanges)
        entities.append(contentsOf: removedChatChanges)
        entities.append(contentsOf: messageChanges)

        return DeviceSyncCollectedChanges(
            entities: entities,
            timePoint: timePoint
        )
    }
}

private extension DeviceSyncOutgoingChangesCollector {
    func collectDeviceChanges(after cutoff: Date?) async throws -> [Chat.DeviceSyncEntity] {
        let deviceFilter = cutoff.map { NSPredicate.devicesCreatedAfter($0) }
        let devices = try await deviceRepositoryFactory
            .createRepository(forFilter: deviceFilter)
            .fetchAllOperation(with: .init())
            .asyncExecute()

        let wireDevices = devices.map { Chat.DeviceSyncWireDevice(from: $0) }
        return wireDevices.isEmpty ? [] : [.devices(wireDevices)]
    }

    func collectContactChanges(after cutoff: Date?) async throws -> [Chat.DeviceSyncEntity] {
        let predicates: [NSPredicate] = [
            cutoff.map { NSPredicate.contactsAcceptedAfter($0) },
            NSPredicate.acceptedContacts
        ].compactMap { $0 }

        let contacts = try await contactRepositoryFactory
            .createRepository(forFilter: NSCompoundPredicate(andPredicateWithSubpredicates: predicates))
            .fetchAllOperation(with: .init())
            .asyncExecute()

        let chatIds = contacts.compactMap { contact -> Chat.DeviceSyncChatId? in
            let mode = messageExchangeModeProvider.mode(forSignKeyId: contact.ownKeyId.signKeyId)
            guard mode == .multidevice else { return nil }
            return .contact(accountId: contact.accountId)
        }

        return chatIds.isEmpty ? [] : [.chatsAdded(chatIds)]
    }

    func collectRemovedChatChanges(after cutoff: Date?) async throws -> [Chat.DeviceSyncEntity] {
        let removedFilter = cutoff.map { NSPredicate.removedChatsAfter($0) }
        let removedChats = try await removedChatRepositoryFactory
            .createRepository(forFilter: removedFilter)
            .fetchAllOperation(with: .init())
            .asyncExecute()

        let chatIds = removedChats.map { Chat.DeviceSyncChatId.contact(accountId: $0.accountId) }
        return chatIds.isEmpty ? [] : [.chatsRemoved(chatIds)]
    }

    func collectMessageChanges(since checkpoint: UInt64?) async throws -> [Chat.DeviceSyncEntity] {
        let multideviceSignKeyIds = messageExchangeModeProvider.multideviceSignKeyIds
        guard !multideviceSignKeyIds.isEmpty else { return [] }

        let messageFilter = NSPredicate.syncableMessagesForAcceptedContacts(
            since: checkpoint,
            ownSignKeyIds: multideviceSignKeyIds
        )
        let messages = try await messageRepositoryFactory
            .createRepository(forFilter: messageFilter)
            .fetchAllOperation(with: .init())
            .asyncExecute()

        let wireMessages = messages.compactMap { Chat.DeviceSyncWireMessage(from: $0) }
        return wireMessages.isEmpty ? [] : [.messages(wireMessages)]
    }
}
