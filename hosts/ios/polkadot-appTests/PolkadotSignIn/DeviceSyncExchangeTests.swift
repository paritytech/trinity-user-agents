@testable import polkadot_app
import Foundation
import SubstrateSdk
import Testing

@Suite(.timeLimit(.minutes(1)))
struct DeviceSyncExchangeTests {
    // MARK: - Lifecycle

    @Test("Closed exchange cannot be resurrected by start")
    func closedExchangeCannotBeRestarted() async throws {
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let session = makeSession(
            peerAccountId: Data(repeating: 2, count: 32),
            flow: dataChannel
        )

        await session.start()
        await dataChannel.waitForObserverSubscriptions()
        await session.close()

        await session.start()

        let counts = await dataChannel.observerSubscriptionCounts
        #expect(counts.updates == 1)
        #expect(counts.acks == 1)
        #expect(counts.states == 1)
    }

    @Test("Terminal flow state fails the exchange")
    func terminalFlowStateFailsExchange() async throws {
        let peerAccountId = Data(repeating: 3, count: 32)
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let failureRecorder = DeviceSyncFailureRecorder()
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            failureRecorder: failureRecorder
        )

        await session.start()
        await dataChannel.sendState(.disconnected)

        let recorded = await failureRecorder.waitForRecordedFailure()
        #expect(recorded.accountId == peerAccountId)
        guard case .disconnected = recorded.failure else {
            Issue.record("Expected disconnected, got \(String(describing: recorded.failure))")
            return
        }
    }

    // MARK: - Incoming Updates

    @Test("Update stream failure notifies failure handler")
    func updateStreamFailureNotifiesFailureHandler() async throws {
        let peerAccountId = Data(repeating: 24, count: 32)
        let dataChannel = MockDeviceSyncPeerConnectionFlow(
            startResult: .success(()),
            failUpdates: true
        )
        let failureRecorder = DeviceSyncFailureRecorder()
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            failureRecorder: failureRecorder
        )

        await session.start()

        let recorded = await failureRecorder.waitForRecordedFailure()
        #expect(recorded.accountId == peerAccountId)

        guard case .connectionFailed = recorded.failure else {
            Issue.record("Expected connectionFailed, got \(String(describing: recorded.failure))")
            return
        }

        await session.close()
    }

    @Test("Incoming messages are persisted and acked")
    func incomingMessagesArePersistedAndAcked() async throws {
        let peerAccountId = Data(repeating: 30, count: 32)
        let messageRepositoryFactory = MockChatMessageRepositoryFactory()
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let localMessage = Chat.LocalMessage(
            messageId: "synced-message",
            chatId: .person(Data(repeating: 31, count: 32)),
            origin: .user,
            creationSource: .deviceSync,
            status: .outgoing(.sent),
            timestamp: 7_891,
            content: .text("hello"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let wireMessage = try #require(Chat.DeviceSyncWireMessage(from: localMessage))

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            messageRepositoryFactory: messageRepositoryFactory
        )

        await session.start()
        await dataChannel.emitUpdate(Chat.DeviceSyncUpdate(
            id: 45,
            entities: [.messages([wireMessage])],
            timePoint: 7_891
        ))

        let sentAck = await dataChannel.waitForSentAck()
        let savedMessages = try await messageRepositoryFactory.fetchMessages()
        let savedMessage = try #require(savedMessages.first { $0.messageId == localMessage.messageId })
        #expect(savedMessage.creationSource == .deviceSync)
        #expect(sentAck == Chat.DeviceSyncUpdateAck(id: 45))

        await session.close()
    }

    @Test("Failed incoming entity is skipped while remaining entities are applied and acked")
    func failedIncomingEntityDoesNotBlockRemainingEntities() async throws {
        let peerAccountId = Data(repeating: 38, count: 32)
        let contactAccountId = Data(repeating: 39, count: 32)
        let messageRepositoryFactory = MockChatMessageRepositoryFactory()
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let localMessage = Chat.LocalMessage(
            messageId: "message-after-failed-entity",
            chatId: .person(contactAccountId),
            origin: .user,
            creationSource: .deviceSync,
            status: .outgoing(.sent),
            timestamp: 7_892,
            content: .text("still applied"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let wireMessage = try #require(Chat.DeviceSyncWireMessage(from: localMessage))
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            remoteContactResolver: MockRemoteContactResolver(
                result: .failure(MockDeviceSyncError.entityApplicationFailed)
            ),
            messageRepositoryFactory: messageRepositoryFactory
        )

        await session.start()
        await dataChannel.emitUpdate(Chat.DeviceSyncUpdate(
            id: 49,
            entities: [
                .chatsAdded([.contact(accountId: contactAccountId)]),
                .messages([wireMessage])
            ],
            timePoint: 7_892
        ))

        let sentAck = await dataChannel.waitForSentAck()
        let savedMessages = try await messageRepositoryFactory.fetchMessages()

        #expect(savedMessages.contains { $0.messageId == localMessage.messageId })
        #expect(sentAck == Chat.DeviceSyncUpdateAck(id: 49))

        await session.close()
    }

    @Test("Incoming synced message does not overwrite local device message")
    func incomingSyncedMessageDoesNotOverwriteLocalDeviceMessage() async throws {
        let peerAccountId = Data(repeating: 34, count: 32)
        let contactAccountId = Data(repeating: 35, count: 32)
        let messageRepositoryFactory = MockChatMessageRepositoryFactory()
        let existingMessage = Chat.LocalMessage(
            messageId: "local-device-message",
            chatId: .person(contactAccountId),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.new),
            timestamp: 9_001,
            content: .text("local content"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
        try await messageRepositoryFactory.save([existingMessage])

        let syncedMessage = Chat.LocalMessage(
            messageId: existingMessage.messageId,
            chatId: .person(contactAccountId),
            origin: .user,
            creationSource: .deviceSync,
            status: .outgoing(.delivered),
            timestamp: 9_002,
            content: .text("synced content"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let wireMessage = try #require(Chat.DeviceSyncWireMessage(from: syncedMessage))
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            messageRepositoryFactory: messageRepositoryFactory
        )

        await session.start()
        await dataChannel.emitUpdate(Chat.DeviceSyncUpdate(
            id: 47,
            entities: [.messages([wireMessage])],
            timePoint: 9_002
        ))

        let sentAck = await dataChannel.waitForSentAck()
        let savedMessage = try #require(
            try await messageRepositoryFactory.fetchMessages().first { $0.messageId == existingMessage.messageId }
        )

        #expect(sentAck == Chat.DeviceSyncUpdateAck(id: 47))
        #expect(savedMessage.creationSource == .localDevice)
        #expect(savedMessage.status == .outgoing(.new))
        #expect(savedMessage.content == .text("local content"))

        await session.close()
    }

    @Test("Incoming synced message overwrites device sync message")
    func incomingSyncedMessageOverwritesDeviceSyncMessage() async throws {
        let peerAccountId = Data(repeating: 36, count: 32)
        let contactAccountId = Data(repeating: 37, count: 32)
        let messageRepositoryFactory = MockChatMessageRepositoryFactory()
        let existingMessage = Chat.LocalMessage(
            messageId: "device-sync-message",
            chatId: .person(contactAccountId),
            origin: .user,
            creationSource: .deviceSync,
            status: .outgoing(.delivered),
            timestamp: 10_001,
            content: .text("old content"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        try await messageRepositoryFactory.save([existingMessage])

        let syncedMessage = Chat.LocalMessage(
            messageId: existingMessage.messageId,
            chatId: .person(contactAccountId),
            origin: .user,
            creationSource: .deviceSync,
            status: .outgoing(.sent),
            timestamp: 10_002,
            content: .text("updated content"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let wireMessage = try #require(Chat.DeviceSyncWireMessage(from: syncedMessage))
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            messageRepositoryFactory: messageRepositoryFactory
        )

        await session.start()
        await dataChannel.emitUpdate(Chat.DeviceSyncUpdate(
            id: 48,
            entities: [.messages([wireMessage])],
            timePoint: 10_002
        ))

        let sentAck = await dataChannel.waitForSentAck()
        let savedMessage = try #require(
            try await messageRepositoryFactory.fetchMessages().first { $0.messageId == existingMessage.messageId }
        )

        #expect(sentAck == Chat.DeviceSyncUpdateAck(id: 48))
        #expect(savedMessage.creationSource == .deviceSync)
        #expect(savedMessage.status == .outgoing(.sent))
        #expect(savedMessage.content == .text("updated content"))

        await session.close()
    }

    @Test("Incoming removed chats delete contacts, store tombstones, and ack")
    func incomingRemovedChatsDeleteContactsStoreTombstonesAndAck() async throws {
        let peerAccountId = Data(repeating: 32, count: 32)
        let removedAccountId = Data(repeating: 33, count: 32)
        let updateTimePoint: UInt64 = 8_912
        let contactRepositoryFactory = MockChatContactRepositoryFactory()
        let removedChatRepositoryFactory = MockRemovedChatRepositoryFactory()
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            contactRepositoryFactory: contactRepositoryFactory,
            removedChatRepositoryFactory: removedChatRepositoryFactory
        )

        await session.start()
        await dataChannel.emitUpdate(Chat.DeviceSyncUpdate(
            id: 46,
            entities: [.chatsRemoved([.contact(accountId: removedAccountId)])],
            timePoint: updateTimePoint
        ))

        let sentAck = await dataChannel.waitForSentAck()
        let deletedIds = contactRepositoryFactory.deletedIds
        let tombstones = try await removedChatRepositoryFactory.fetchRemovedChats()

        #expect(deletedIds == [removedAccountId.toHex()])
        #expect(tombstones.contains {
            $0.accountId == removedAccountId &&
                $0.removedAt == Date.fromChatTimestamp(updateTimePoint)
        })
        #expect(sentAck == Chat.DeviceSyncUpdateAck(id: 46))

        await session.close()
    }

    @Test("Ack send failure notifies failure handler")
    func ackSendFailureNotifiesFailureHandler() async throws {
        let peerAccountId = Data(repeating: 6, count: 32)
        let dataChannel = MockDeviceSyncPeerConnectionFlow(
            startResult: .success(()),
            sendAckResult: .failure(MockDeviceSyncError.sendAckFailed)
        )
        let failureRecorder = DeviceSyncFailureRecorder()

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            failureRecorder: failureRecorder
        )

        await session.start()

        let update = Chat.DeviceSyncUpdate(
            id: 44,
            entities: [.devices([])],
            timePoint: Date().toChatTimestamp()
        )
        await dataChannel.emitUpdate(update)

        let recorded = await failureRecorder.waitForRecordedFailure()
        #expect(recorded.accountId == peerAccountId)

        guard case .sendFailed = recorded.failure else {
            Issue.record("Expected sendFailed, got \(String(describing: recorded.failure))")
            return
        }

        #expect(await !dataChannel.didClose)
    }
}
