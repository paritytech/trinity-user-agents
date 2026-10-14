import Foundation
import AsyncExtensions
import MessageExchangeKit
import Operation_iOS
import StructuredConcurrency

protocol DeviceSyncExchanging: AnyObject, Sendable {
    func start() async
    func close() async
}

actor DeviceSyncExchange {
    static let defaultAckTimeout: Duration = .seconds(30)

    private let peerStatementAccountId: Data

    private let flow: any DeviceSyncPeerConnectionFlowing
    private let dependencies: DeviceSyncExchangeDependencies
    private let incomingUpdateApplier: DeviceSyncIncomingUpdateApplier
    private let outgoingChangesCollector: DeviceSyncOutgoingChangesCollector
    private let peerLogger: LoggerProtocol
    private let behavior: DeviceSyncExchangeBehavior
    private let handlers: DeviceSyncExchangeHandlers

    private var sessionTask: Task<Void, Never>?
    private var ackTimeoutTasks: [UInt32: Task<Void, Never>] = [:]

    private var snapshotAccounting = DeviceSyncSnapshotAccounting()
    private var outgoingCheckpoint: UInt64?
    private var isDataChannelConnected = false
    private var isPushInProgress = false
    private var needsDeferredPush = false
    private var isFailed = false

    init(
        peer: DeviceSyncExchangePeer,
        flow: any DeviceSyncPeerConnectionFlowing,
        dependencies: DeviceSyncExchangeDependencies,
        peerLogger: LoggerProtocol,
        behavior: DeviceSyncExchangeBehavior,
        handlers: DeviceSyncExchangeHandlers
    ) {
        peerStatementAccountId = peer.statementAccountId
        outgoingCheckpoint = peer.initialCheckpoint
        self.flow = flow
        self.dependencies = dependencies
        self.behavior = behavior
        self.handlers = handlers
        incomingUpdateApplier = dependencies.makeIncomingUpdateApplier(
            flow: flow,
            peerLogger: peerLogger
        )
        outgoingChangesCollector = dependencies.makeOutgoingChangesCollector()
        self.peerLogger = peerLogger
    }

    deinit {
        peerLogger.debug("Deinit for \(peerStatementAccountId.toHex())")
    }

    func start() async {
        guard !isFailed else { return }
        await startExchangeTasks()
    }

    func close() async {
        let sessionTask = sessionTask
        self.sessionTask = nil
        sessionTask?.cancel()
        cancelAllAckTimeouts()

        isDataChannelConnected = false
        isPushInProgress = false
        isFailed = true
        snapshotAccounting.reset()

        await sessionTask?.value
    }
}

extension DeviceSyncExchange: DeviceSyncExchanging {}

private extension DeviceSyncExchange {
    func startExchangeTasks() async {
        guard sessionTask == nil else { return }

        let updateIterator = await flow.updates.makeAsyncIterator()
        let ackIterator = await flow.acks.makeAsyncIterator()
        let stateIterator = await flow.states.makeAsyncIterator()

        sessionTask = Task { [weak self] in
            await withTaskGroup(of: Void.self) { group in
                group.addTask { await self?.receiveUpdates(iterator: updateIterator) }
                group.addTask { await self?.receiveAcks(iterator: ackIterator) }
                group.addTask { await self?.observeLocalChanges() }
                group.addTask { await self?.observeDataChannelState(iterator: stateIterator) }
            }
        }
    }

    func failSession(_ failure: DeviceSyncConnectionFailure) async {
        guard !isFailed else { return }

        peerLogger.error("Device sync session failed for \(peerStatementAccountId.toHex()): \(failure)")

        isFailed = true
        isDataChannelConnected = false
        isPushInProgress = false
        snapshotAccounting.reset()

        sessionTask?.cancel()
        cancelAllAckTimeouts()

        await handlers.failure(failure)
    }

    func observeDataChannelState(
        iterator initialIterator: AnyAsyncIterator<DeviceSyncDataChannelState>
    ) async {
        var iterator = initialIterator

        do {
            while let state = try await iterator.next() {
                guard !Task.isCancelled else { return }

                switch state {
                case .connected:
                    let wasConnected = isDataChannelConnected
                    isDataChannelConnected = true
                    if !wasConnected {
                        await requestPushUpdate()
                    }
                case .disconnected:
                    isDataChannelConnected = false
                    await failSession(.disconnected)
                }
            }
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error("Data channel state observation failed: \(error)")
            await failSession(.disconnected)
        }
    }

    func observeLocalChanges() async {
        await withTaskGroup(of: Void.self) { group in
            group.addTask { await self.observeContactChanges() }
            group.addTask { await self.observeMessageChanges() }
        }
    }

    func observeContactChanges() async {
        do {
            for try await _ in dependencies.contactDataProviderFactory
                .subscribeContactsWithPredicate(.acceptedContacts) {
                guard !Task.isCancelled else { return }
                await requestPushUpdate()
            }
            guard !Task.isCancelled else { return }
            peerLogger.warning(
                "Contact sync observation ended; automatic contact-triggered sync is degraded"
            )
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error(
                "Contact sync observation failed; automatic contact-triggered sync is degraded: \(error)"
            )
        }
    }

    func observeMessageChanges() async {
        let multideviceSignKeyIds = dependencies.messageExchangeModeProvider.multideviceSignKeyIds

        guard !multideviceSignKeyIds.isEmpty else { return }

        do {
            for try await _ in dependencies.messageDataProviderFactory.subscribeMessages(
                with: .syncableMessagesForAcceptedContacts(
                    since: nil,
                    ownSignKeyIds: multideviceSignKeyIds
                )
            ) {
                guard !Task.isCancelled else { return }
                await requestPushUpdate()
            }
            guard !Task.isCancelled else { return }
            peerLogger.warning(
                "Message sync observation ended; automatic message-triggered sync is degraded"
            )
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error(
                "Message sync observation failed; automatic message-triggered sync is degraded: \(error)"
            )
        }
    }

    func receiveUpdates(
        iterator initialIterator: AnyAsyncIterator<Chat.DeviceSyncUpdate>
    ) async {
        var iterator = initialIterator

        do {
            while let update = try await iterator.next() {
                guard !Task.isCancelled else { return }
                await processSyncUpdate(update)
            }
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error("Update receive failed: \(error)")
            await failSession(.connectionFailed(error))
        }
    }

    func receiveAcks(
        iterator initialIterator: AnyAsyncIterator<Chat.DeviceSyncUpdateAck>
    ) async {
        var iterator = initialIterator

        do {
            while let ack = try await iterator.next() {
                guard !Task.isCancelled else { return }
                await processSyncUpdateAck(ack)
            }
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error("Ack receive failed: \(error)")
            await failSession(.connectionFailed(error))
        }
    }

    func processSyncUpdate(_ update: Chat.DeviceSyncUpdate) async {
        do {
            try await incomingUpdateApplier.apply(update)
        } catch {
            peerLogger.error("Failed to send ack: \(error)")
            await failSession(.sendFailed(error))
        }
    }

    func requestPushUpdate() async {
        guard isDataChannelConnected else {
            needsDeferredPush = true
            return
        }

        guard !isPushInProgress, !snapshotAccounting.hasPendingUpdates else {
            needsDeferredPush = true
            return
        }

        needsDeferredPush = false
        isPushInProgress = true
        await pushNextUpdate()
        isPushInProgress = false

        if snapshotAccounting.consumeDeferredTimeout() {
            await retryFromDurableCheckpoint()
            return
        }

        if !snapshotAccounting.hasPendingUpdates, needsDeferredPush {
            await requestPushUpdate()
        }
    }

    func pushNextUpdate() async {
        let checkpoint = outgoingCheckpoint
        let changes: DeviceSyncCollectedChanges

        do {
            changes = try await outgoingChangesCollector.collectEntities(since: checkpoint)
        } catch {
            await failSession(.collectionFailed(error))
            return
        }

        let timePoint = changes.timePoint
        var sentIds: [UInt32] = []

        guard !changes.entities.isEmpty else {
            peerLogger.debug("No pending updates to send")
            await advanceOutgoingUpdateTime(to: timePoint)
            return
        }

        do {
            let batches = try behavior.entityChunker.chunk(changes.entities)

            guard !batches.isEmpty else {
                throw DeviceSyncEntityChunkingError.emptyResult
            }

            let updates: [Chat.DeviceSyncUpdate] = batches.map { batch in
                Chat.DeviceSyncUpdate(
                    id: dependencies.updateIdProvider.nextId(),
                    entities: batch,
                    timePoint: timePoint
                )
            }

            snapshotAccounting.beginSnapshot(updates: updates, timePoint: timePoint)

            for update in updates {
                snapshotAccounting.registerPending(update)
                sentIds.append(update.id)

                try await flow.sendUpdate(update)
                startAckTimeout(for: update.id)
                peerLogger.debug(
                    "Sent SyncUpdate id=\(update.id) groups=\(update.entities.count) " +
                        "entities=\(update.entities.syncLogSummary) of \(updates.count)"
                )
            }
        } catch {
            sentIds.forEach(cancelAckTimeout)
            snapshotAccounting.rollBackSnapshot(sentIds: sentIds)
            peerLogger.error("Failed to send pending updates: \(error)")
            await failSession(.sendFailed(error))
        }
    }

    func processSyncUpdateAck(_ ack: Chat.DeviceSyncUpdateAck) async {
        let acknowledgement = snapshotAccounting.acknowledge(updateId: ack.id)

        switch acknowledgement {
        case .unknown:
            peerLogger.warning("Received ack for unknown update id=\(ack.id)")
            return
        case .inconsistent:
            peerLogger.error("Ignoring ack id=\(ack.id) without active snapshot accounting")
            return
        case let .recorded(update):
            cancelAckTimeout(for: ack.id)
            logAcknowledgement(ack, update: update)
            return
        case let .snapshotCompleted(update, timePoint):
            cancelAckTimeout(for: ack.id)
            logAcknowledgement(ack, update: update)
            await advanceOutgoingUpdateTime(to: timePoint)
        }

        if needsDeferredPush {
            await requestPushUpdate()
        }
    }

    func startAckTimeout(for updateId: UInt32) {
        ackTimeoutTasks[updateId]?.cancel()
        let ackTimeout = behavior.ackTimeout
        let sleep = behavior.sleep
        ackTimeoutTasks[updateId] = Task { [weak self] in
            do {
                try await sleep(ackTimeout)
            } catch {
                return
            }

            guard !Task.isCancelled else { return }
            await self?.handleAckTimeout(for: updateId)
        }
    }

    func cancelAckTimeout(for updateId: UInt32) {
        ackTimeoutTasks.removeValue(forKey: updateId)?.cancel()
    }

    func cancelAllAckTimeouts() {
        ackTimeoutTasks.values.forEach { $0.cancel() }
        ackTimeoutTasks.removeAll()
    }

    func handleAckTimeout(for updateId: UInt32) async {
        ackTimeoutTasks.removeValue(forKey: updateId)

        guard snapshotAccounting.containsPending(updateId: updateId) else { return }

        if isPushInProgress {
            peerLogger.warning("Ack timed out for update id=\(updateId); deferring retry until push finishes")
            snapshotAccounting.deferTimeout()
            return
        }

        peerLogger.warning("Ack timed out for update id=\(updateId); retrying from durable checkpoint")
        await retryFromDurableCheckpoint()
    }

    func retryFromDurableCheckpoint() async {
        guard !isFailed else { return }

        snapshotAccounting.reset()
        cancelAllAckTimeouts()
        needsDeferredPush = true
        await requestPushUpdate()
    }

    func advanceOutgoingUpdateTime(to timePoint: UInt64) async {
        // Incoming entity application is idempotent, so a failed durable save may safely cause
        // this range to be collected and sent again after the connection is replaced.
        outgoingCheckpoint = timePoint
        let update = Chat.OutgoingUpdateTimeUpdate(
            statementAccountId: peerStatementAccountId,
            outgoingUpdateTime: timePoint
        )

        do {
            let repository = dependencies.outgoingUpdateTimeRepositoryFactory.createRepository(forFilter: nil)
            try await repository.saveOperation({ [update] }, { [] }).asyncExecute()
            peerLogger.debug("Advanced outgoingUpdateTime to \(timePoint)")
        } catch {
            peerLogger.error("Failed to advance outgoingUpdateTime: \(error)")
        }
    }

    func logAcknowledgement(
        _ ack: Chat.DeviceSyncUpdateAck,
        update: Chat.DeviceSyncUpdate
    ) {
        peerLogger.debug("Received SyncUpdateAck id=\(ack.id) entities=\(update.entities.syncLogSummary)")
    }
}
