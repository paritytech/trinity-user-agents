@testable import polkadot_app
import AsyncExtensions
import Foundation
import MessageExchangeKit

actor MockDeviceSyncPeerConnectionFlow: DeviceSyncPeerConnectionFlowing {
    private nonisolated let failUpdates: Bool
    private nonisolated let sentUpdateEvents = DeviceSyncTestEventRecorder<Chat.DeviceSyncUpdate>()
    private nonisolated let sentAckEvents = DeviceSyncTestEventRecorder<Chat.DeviceSyncUpdateAck>()
    private nonisolated let observerEvents = DeviceSyncTestEventRecorder<Void>()
    private let startResult: Result<Void, Error>
    private let sendUpdateResult: Result<Void, Error>
    private let sendAckResult: Result<Void, Error>
    private let suspendConnect: Bool
    private let suspendUpdateAtIndex: Int?
    private nonisolated let blockedSendEvents = DeviceSyncTestEventRecorder<Void>()
    private(set) var didClose = false
    private(set) var sentAcks = [Chat.DeviceSyncUpdateAck]()
    private(set) var sentUpdates = [Chat.DeviceSyncUpdate]()
    private var stateContinuations = [AsyncStream<DeviceSyncDataChannelState>.Continuation]()
    private var updateContinuations = [AsyncStream<Chat.DeviceSyncUpdate>.Continuation]()
    private var ackContinuations = [AsyncStream<Chat.DeviceSyncUpdateAck>.Continuation]()
    private var connectContinuation: CheckedContinuation<Void, Error>?
    private var blockedSendContinuation: CheckedContinuation<Void, Never>?
    private var sendIndex = 0

    init(
        startResult: Result<Void, Error>,
        sendUpdateResult: Result<Void, Error> = .success(()),
        sendAckResult: Result<Void, Error> = .success(()),
        suspendConnect: Bool = false,
        suspendUpdateAtIndex: Int? = nil,
        failUpdates: Bool = false
    ) {
        self.failUpdates = failUpdates
        self.startResult = startResult
        self.sendUpdateResult = sendUpdateResult
        self.sendAckResult = sendAckResult
        self.suspendConnect = suspendConnect
        self.suspendUpdateAtIndex = suspendUpdateAtIndex
    }

    var updates: AnyAsyncSequence<Chat.DeviceSyncUpdate> {
        if failUpdates {
            return AsyncThrowingStream<Chat.DeviceSyncUpdate, Error> { continuation in
                continuation.finish(throwing: MockDeviceSyncError.updateStreamFailed)
            }.eraseToAnyAsyncSequence()
        }

        return AsyncStream<Chat.DeviceSyncUpdate> { continuation in
            addUpdateContinuation(continuation)
        }.eraseToAnyAsyncSequence()
    }

    var acks: AnyAsyncSequence<Chat.DeviceSyncUpdateAck> {
        AsyncStream<Chat.DeviceSyncUpdateAck> { continuation in
            addAckContinuation(continuation)
        }.eraseToAnyAsyncSequence()
    }

    var states: AnyAsyncSequence<DeviceSyncDataChannelState> {
        AsyncStream<DeviceSyncDataChannelState> { continuation in
            addStateContinuation(continuation)
        }.eraseToAnyAsyncSequence()
    }

    func start() async throws {
        try startResult.get()

        guard suspendConnect else { return }

        try await withCheckedThrowingContinuation { continuation in
            connectContinuation = continuation
        }
    }

    func sendUpdate(_ update: Chat.DeviceSyncUpdate) async throws {
        try sendUpdateResult.get()

        let currentIndex = sendIndex
        sendIndex += 1
        if currentIndex == suspendUpdateAtIndex {
            blockedSendEvents.record(())
            await withCheckedContinuation { continuation in
                blockedSendContinuation = continuation
            }
        }

        sentUpdates.append(update)
        sentUpdateEvents.record(update)
    }

    func sendAck(_ ack: Chat.DeviceSyncUpdateAck) async throws {
        try sendAckResult.get()
        sentAcks.append(ack)
        sentAckEvents.record(ack)
    }

    func cancel() async {
        didClose = true
        connectContinuation?.resume(throwing: MockDeviceSyncError.connectCancelled)
        connectContinuation = nil
    }

    nonisolated func waitForSentUpdate() async -> Chat.DeviceSyncUpdate {
        let updates = await sentUpdateEvents.waitForCount(1)
        return updates[0]
    }

    nonisolated func waitForSentUpdateCount(_ count: Int) async -> [Chat.DeviceSyncUpdate] {
        await sentUpdateEvents.waitForCount(count)
    }

    nonisolated func waitUntilSendBlocked() async {
        _ = await blockedSendEvents.waitForCount(1)
    }

    func resumeBlockedSend() {
        blockedSendContinuation?.resume()
        blockedSendContinuation = nil
    }

    nonisolated func waitForSentAck() async -> Chat.DeviceSyncUpdateAck {
        let acks = await sentAckEvents.waitForCount(1)
        return acks[0]
    }

    nonisolated func waitForObserverSubscriptions() async {
        _ = await observerEvents.waitForCount(3)
    }

    var observerSubscriptionCounts: (updates: Int, acks: Int, states: Int) {
        (updateContinuations.count, ackContinuations.count, stateContinuations.count)
    }

    func sendState(_ state: DeviceSyncDataChannelState) {
        stateContinuations.forEach { $0.yield(state) }
    }

    func emitUpdate(_ update: Chat.DeviceSyncUpdate) {
        updateContinuations.forEach { $0.yield(update) }
    }

    func emitAck(_ ack: Chat.DeviceSyncUpdateAck) {
        ackContinuations.forEach { $0.yield(ack) }
    }

    private func addStateContinuation(_ continuation: AsyncStream<DeviceSyncDataChannelState>.Continuation) {
        stateContinuations.append(continuation)
        observerEvents.record(())
    }

    private func addUpdateContinuation(_ continuation: AsyncStream<Chat.DeviceSyncUpdate>.Continuation) {
        updateContinuations.append(continuation)
        observerEvents.record(())
    }

    private func addAckContinuation(_ continuation: AsyncStream<Chat.DeviceSyncUpdateAck>.Continuation) {
        ackContinuations.append(continuation)
        observerEvents.record(())
    }
}
