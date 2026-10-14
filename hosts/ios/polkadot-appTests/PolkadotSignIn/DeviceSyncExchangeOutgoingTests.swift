@testable import polkadot_app
import Foundation
import SubstrateSdk
import Testing

extension DeviceSyncExchangeTests {
    // MARK: - Outgoing Updates

    @Test("Outgoing checkpoint advances only after ack")
    func outgoingCheckpointAdvancesOnlyAfterAck() async throws {
        let peerAccountId = Data(repeating: 13, count: 32)
        let createdAt = Date(timeIntervalSince1970: 2_345.678)
        let deviceRepositoryFactory = MockLocalDeviceRepositoryFactory()
        let outgoingUpdateTimeRepositoryFactory = MockOutgoingUpdateTimeRepositoryFactory()
        try await deviceRepositoryFactory.save([
            Chat.LocalDevice(
                statementAccountId: Data(repeating: 14, count: 32),
                encryptionPublicKey: Data(repeating: 15, count: 65),
                hostName: "acked-device",
                createdAt: createdAt,
                hostVersion: nil,
                osType: nil,
                osVersion: nil,
                outgoingUpdateTime: nil
            )
        ])

        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            deviceRepositoryFactory: deviceRepositoryFactory,
            contactRepositoryFactory: MockChatContactRepositoryFactory(),
            messageRepositoryFactory: MockChatMessageRepositoryFactory(),
            removedChatRepositoryFactory: MockRemovedChatRepositoryFactory(),
            outgoingUpdateTimeRepositoryFactory: outgoingUpdateTimeRepositoryFactory
        )

        let startedAt = Date().toChatTimestamp()
        await session.start()
        await dataChannel.sendState(.connected)
        let sentUpdate = await dataChannel.waitForSentUpdate()
        let sentAt = Date().toChatTimestamp()

        #expect(sentUpdate.timePoint >= startedAt)
        #expect(sentUpdate.timePoint <= sentAt)
        #expect(outgoingUpdateTimeRepositoryFactory.savedUpdates.isEmpty)

        await dataChannel.emitAck(Chat.DeviceSyncUpdateAck(id: sentUpdate.id))
        let savedUpdate = await outgoingUpdateTimeRepositoryFactory.waitForSavedUpdate()

        #expect(savedUpdate.statementAccountId == peerAccountId)
        #expect(savedUpdate.outgoingUpdateTime == sentUpdate.timePoint)

        await session.close()
    }

    @Test("Ack timeout retries from the durable checkpoint without closing the connection")
    func ackTimeoutRetriesWithoutClosingConnection() async throws {
        let peerAccountId = Data(repeating: 18, count: 32)
        let deviceRepositoryFactory = MockLocalDeviceRepositoryFactory()
        try await deviceRepositoryFactory.save([
            Chat.LocalDevice(
                statementAccountId: Data(repeating: 19, count: 32),
                encryptionPublicKey: Data(repeating: 20, count: 65),
                hostName: "timeout-device",
                createdAt: Date(timeIntervalSince1970: 4_567.89),
                hostVersion: nil,
                osType: nil,
                osVersion: nil,
                outgoingUpdateTime: nil
            )
        ])
        let dataChannel = MockDeviceSyncPeerConnectionFlow(startResult: .success(()))
        let failureRecorder = DeviceSyncFailureRecorder()
        let sleeper = ManualDeviceSyncSleeper()

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            failureRecorder: failureRecorder,
            deviceRepositoryFactory: deviceRepositoryFactory,
            contactRepositoryFactory: MockChatContactRepositoryFactory(),
            messageRepositoryFactory: MockChatMessageRepositoryFactory(),
            removedChatRepositoryFactory: MockRemovedChatRepositoryFactory(),
            ackTimeout: .seconds(30),
            sleep: { duration in try await sleeper.sleep(for: duration) }
        )

        await session.start()
        await dataChannel.sendState(.connected)
        let sentUpdate = await dataChannel.waitForSentUpdate()
        await sleeper.waitUntilSleeping()
        sleeper.advance()

        let updates = await dataChannel.waitForSentUpdateCount(2)
        #expect(updates.count >= 2)
        #expect(updates[1].timePoint >= sentUpdate.timePoint)
        #expect(await failureRecorder.recordedFailure == nil)
        #expect(await !dataChannel.didClose)
        await session.close()
    }

    @Test("Ack timeout during a blocked multi-chunk push retries without advancing its checkpoint")
    func ackTimeoutDuringBlockedPushPreservesSnapshotAccounting() async throws {
        let peerAccountId = Data(repeating: 31, count: 32)
        let devices = (1 ... 3).map { index in
            Chat.LocalDevice(
                statementAccountId: Data(repeating: UInt8(index), count: 32),
                encryptionPublicKey: Data(repeating: UInt8(index), count: 65),
                hostName: "device-\(index)",
                createdAt: Date(timeIntervalSince1970: TimeInterval(index)),
                hostVersion: nil,
                osType: nil,
                osVersion: nil,
                outgoingUpdateTime: nil
            )
        }
        let deviceRepositoryFactory = MockLocalDeviceRepositoryFactory()
        try await deviceRepositoryFactory.save(devices)

        let dataChannel = MockDeviceSyncPeerConnectionFlow(
            startResult: .success(()),
            suspendUpdateAtIndex: 1
        )
        let outgoingUpdateTimeRepositoryFactory = MockOutgoingUpdateTimeRepositoryFactory()
        let sleeper = ManualDeviceSyncSleeper()
        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            deviceRepositoryFactory: deviceRepositoryFactory,
            outgoingUpdateTimeRepositoryFactory: outgoingUpdateTimeRepositoryFactory,
            ackTimeout: .seconds(30),
            sleep: { duration in try await sleeper.sleep(for: duration) },
            entityChunker: DeviceSyncEntityChunker(maxPayloadSize: 110)
        )

        await session.start()
        await dataChannel.sendState(.connected)
        _ = await dataChannel.waitForSentUpdate()
        await sleeper.waitUntilSleeping()
        await dataChannel.waitUntilSendBlocked()

        sleeper.advance()
        #expect(await sleeper.waitForOutcome() == .completed)
        await Task.yield()

        await dataChannel.resumeBlockedSend()
        let updates = await dataChannel.waitForSentUpdateCount(6)
        #expect(updates.count == 6)
        #expect(outgoingUpdateTimeRepositoryFactory.savedUpdates.isEmpty)

        for update in updates.suffix(3) {
            await dataChannel.emitAck(Chat.DeviceSyncUpdateAck(id: update.id))
        }

        let savedUpdate = await outgoingUpdateTimeRepositoryFactory.waitForSavedUpdate()
        #expect(savedUpdate.outgoingUpdateTime == updates.last?.timePoint)
        await session.close()
    }

    @Test("Send update failure notifies failure handler")
    func sendUpdateFailureNotifiesFailureHandler() async throws {
        let peerAccountId = Data(repeating: 21, count: 32)
        let deviceRepositoryFactory = MockLocalDeviceRepositoryFactory()
        let outgoingUpdateTimeRepositoryFactory = MockOutgoingUpdateTimeRepositoryFactory()
        try await deviceRepositoryFactory.save([
            Chat.LocalDevice(
                statementAccountId: Data(repeating: 22, count: 32),
                encryptionPublicKey: Data(repeating: 23, count: 65),
                hostName: "send-failure-device",
                createdAt: Date(timeIntervalSince1970: 5_678.91),
                hostVersion: nil,
                osType: nil,
                osVersion: nil,
                outgoingUpdateTime: nil
            )
        ])
        let dataChannel = MockDeviceSyncPeerConnectionFlow(
            startResult: .success(()),
            sendUpdateResult: .failure(MockDeviceSyncError.sendUpdateFailed)
        )
        let failureRecorder = DeviceSyncFailureRecorder()

        let session = makeSession(
            peerAccountId: peerAccountId,
            flow: dataChannel,
            failureRecorder: failureRecorder,
            deviceRepositoryFactory: deviceRepositoryFactory,
            contactRepositoryFactory: MockChatContactRepositoryFactory(),
            messageRepositoryFactory: MockChatMessageRepositoryFactory(),
            removedChatRepositoryFactory: MockRemovedChatRepositoryFactory(),
            outgoingUpdateTimeRepositoryFactory: outgoingUpdateTimeRepositoryFactory
        )

        await session.start()
        await dataChannel.sendState(.connected)

        let recorded = await failureRecorder.waitForRecordedFailure()
        #expect(recorded.accountId == peerAccountId)

        guard case .sendFailed = recorded.failure else {
            Issue.record("Expected sendFailed, got \(String(describing: recorded.failure))")
            return
        }

        #expect(outgoingUpdateTimeRepositoryFactory.savedUpdates.isEmpty)
        #expect(await !dataChannel.didClose)
    }
}
