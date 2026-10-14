import Foundation
import Foundation_iOS
import MessageExchangeKit
import StructuredConcurrency
import SubstrateSdk

actor DeviceSyncPeerEngineContext {
    private struct ReplacementRequest {
        let session: DeviceSyncSignalingSession
        let resetSession: Bool
        let connectionId: UUID?
    }

    private enum ReplacementTiming {
        case immediate
        case delayed(Duration)
    }

    private static let connectTimeout: Duration = .seconds(45)

    private let peerStatementAccountId: Data
    private let componentFactory: any DeviceSyncPeerComponentMaking
    private let sessionDelegate: any DeviceSyncPeerSessionDelegating
    private let offerIdPersistenceHandler: @Sendable (String) async throws -> Void
    private let restartDelayPolicy: DeviceSyncRestartDelayPolicy
    private let peerLogger: LoggerProtocol
    private let sleep: DeviceSyncSleep

    private var persistedOfferId: String?
    private var signalingSession: DeviceSyncSignalingSession?
    private var currentFlow: (any DeviceSyncPeerConnectionFlowing)?
    private var currentExchange: (any DeviceSyncExchanging)?
    private var currentConnectionId: UUID?
    private var lifecycleTask: Task<Void, Never>?
    private var currentFlowTask: Task<Void, Never>?
    private var replacementTask: Task<Void, Never>?
    private var restartAttempt = 0
    private var isDisposed = false

    init(
        peerStatementAccountId: Data,
        componentFactory: any DeviceSyncPeerComponentMaking,
        sessionDelegate: any DeviceSyncPeerSessionDelegating,
        persistedOfferId: String?,
        offerIdPersistenceHandler: @escaping @Sendable (String) async throws -> Void,
        restartDelayPolicy: DeviceSyncRestartDelayPolicy,
        peerLogger: LoggerProtocol,
        sleep: @escaping DeviceSyncSleep
    ) {
        self.peerStatementAccountId = peerStatementAccountId
        self.componentFactory = componentFactory
        self.sessionDelegate = sessionDelegate
        self.persistedOfferId = persistedOfferId
        self.offerIdPersistenceHandler = offerIdPersistenceHandler
        self.restartDelayPolicy = restartDelayPolicy
        self.peerLogger = peerLogger
        self.sleep = sleep
    }

    deinit {
        peerLogger.debug("Deinit")
        lifecycleTask?.cancel()
        currentFlowTask?.cancel()
        replacementTask?.cancel()
    }

    func start() {
        guard lifecycleTask == nil, !isDisposed else { return }
        lifecycleTask = Task { [weak self] in await self?.runLifecycle() }
    }

    func dispose() async {
        guard !isDisposed else { return }
        isDisposed = true
        sessionDelegate.finishIncomingEnvelopes()

        let lifecycleTask = lifecycleTask
        let replacementTask = replacementTask
        self.lifecycleTask = nil
        self.replacementTask = nil
        lifecycleTask?.cancel()
        replacementTask?.cancel()

        await closeCurrentConnection()
        await signalingSession?.close()
        signalingSession = nil

        await replacementTask?.value
        await lifecycleTask?.value
    }
}

private extension DeviceSyncPeerEngineContext {
    func runLifecycle() async {
        while !Task.isCancelled, !isDisposed {
            do {
                try await runSignalingSessionLifecycle()
                return
            } catch is CancellationError {
                return
            } catch {
                restartAttempt += 1
                let delay = restartDelayPolicy.delay(forAttempt: restartAttempt)
                peerLogger.error(
                    "Device sync signaling session failed for \(peerStatementAccountId.toHex()): \(error); " +
                        "retrying in \(delay)"
                )
                await closeCurrentConnection()
                await signalingSession?.close()
                signalingSession = nil
                do { try await sleep(delay) } catch { return }
            }
        }
    }

    func runSignalingSessionLifecycle() async throws {
        let delegate = AnyPeerSessionDelegate<Data>(sessionDelegate)
        let session = try await componentFactory.makeSignalingSession(
            delegate: delegate,
            offerIdHandler: { [weak self] offerId in
                guard let self else { throw CancellationError() }
                try await acceptOfferId(offerId)
            }
        )
        guard !Task.isCancelled, !isDisposed else {
            await session.close()
            throw CancellationError()
        }

        signalingSession = session
        try await startConnection(session: session, resetSession: false)

        await consumeIncomingEnvelopes(session: session)
    }

    // Setup failures retry before this single-use stream is consumed. Once consumption starts,
    // the lifecycle is intentionally non-throwing and can only finish through teardown.
    func consumeIncomingEnvelopes(session: DeviceSyncSignalingSession) async {
        for await batch in sessionDelegate.incomingEnvelopes {
            guard !Task.isCancelled, !isDisposed else { return }
            await handleIncoming(batch, session: session)
        }
    }

    func acceptOfferId(_ offerId: String) async throws {
        try await offerIdPersistenceHandler(offerId)
        persistedOfferId = offerId
    }

    func handleIncoming(
        _ batch: [Chat.DeviceSyncSignalingEnvelope],
        session: DeviceSyncSignalingSession
    ) async {
        if let reconnection = await session.reconnection(in: batch) {
            peerLogger.debug("Device sync reconnect accepted for offer \(reconnection.offerId)")
            await replaceConnection(session: session, resetSession: true, replacing: currentConnectionId)
            guard !isDisposed else { return }
            await session.handleIncomingEnvelopes(reconnection.following)
            return
        }

        await session.handleIncomingEnvelopes(batch)
    }

    func startConnection(
        session: DeviceSyncSignalingSession,
        resetSession: Bool
    ) async throws {
        if let offerId = await session.currentReconnectOfferId() ?? persistedOfferId {
            await session.sendReconnected(offerId: offerId)
        }
        if resetSession { await session.resetForReconnection() }

        let connectionId = UUID()
        let flow = componentFactory.makeConnectionFlow(session: session)
        let exchange: any DeviceSyncExchanging

        do {
            exchange = try await componentFactory.makeExchange(
                flow: flow,
                failureHandler: { [weak self] failure in
                    await self?.handleFailure(failure, connectionId: connectionId)
                }
            )
        } catch {
            await flow.cancel()
            throw error
        }

        guard !Task.isCancelled, !isDisposed else {
            await exchange.close()
            await flow.cancel()
            throw CancellationError()
        }

        currentConnectionId = connectionId
        currentFlow = flow
        currentExchange = exchange
        currentFlowTask = Task { [weak self, flow, exchange] in
            do {
                await exchange.start()
                try Task.checkCancellation()
                try await withTimeout(Self.connectTimeout) {
                    try await flow.start()
                }
                try Task.checkCancellation()
                await self?.connectionStarted(connectionId: connectionId)
            } catch is CancellationError {
                return
            } catch {
                await self?.handleFailure(.connectionFailed(error), connectionId: connectionId)
            }
        }
    }

    func connectionStarted(connectionId: UUID) {
        guard currentConnectionId == connectionId, !isDisposed else { return }
        restartAttempt = 0
    }

    func handleFailure(_ failure: DeviceSyncConnectionFailure, connectionId: UUID) {
        guard currentConnectionId == connectionId, !isDisposed else {
            peerLogger.debug("Ignoring stale device sync failure: \(failure)")
            return
        }
        peerLogger.error("Device sync connection failed: \(failure)")
        scheduleRetry(replacing: connectionId)
    }

    func scheduleRetry(replacing connectionId: UUID?) {
        restartAttempt += 1
        guard let request = replacementRequest(replacing: connectionId) else { return }
        enqueueReplacement(
            request,
            timing: .delayed(restartDelayPolicy.delay(forAttempt: restartAttempt))
        )
    }

    func replaceConnection(
        session: DeviceSyncSignalingSession,
        resetSession: Bool,
        replacing connectionId: UUID?
    ) async {
        let request = ReplacementRequest(
            session: session,
            resetSession: resetSession,
            connectionId: connectionId
        )
        let task = enqueueReplacement(request, timing: .immediate)
        await task.value
    }

    @discardableResult
    private func enqueueReplacement(
        _ request: ReplacementRequest,
        timing: ReplacementTiming
    ) -> Task<Void, Never> {
        let previousTask = replacementTask
        previousTask?.cancel()

        let task = Task { [weak self, previousTask] in
            await previousTask?.value
            guard let self, !Task.isCancelled else { return }

            if case let .delayed(delay) = timing {
                do { try await sleep(delay) } catch { return }
            }

            guard !Task.isCancelled else { return }
            await performReplacement(request)
        }
        replacementTask = task
        return task
    }

    private func replacementRequest(replacing connectionId: UUID?) -> ReplacementRequest? {
        guard let session = signalingSession else { return nil }
        return ReplacementRequest(
            session: session,
            resetSession: true,
            connectionId: connectionId
        )
    }

    private func performReplacement(_ request: ReplacementRequest) async {
        guard
            request.connectionId == nil || currentConnectionId == request.connectionId
        else { return }
        await closeCurrentConnection()
        guard !isDisposed, !Task.isCancelled else { return }

        do {
            try await startConnection(session: request.session, resetSession: request.resetSession)
        } catch is CancellationError {
            return
        } catch {
            peerLogger.error("Failed to create device sync connection: \(error)")
            // This replaces and cancels the currently executing replacementTask.
            // Keep it as the final action so this task can finish and unblock
            // the successor waiting for its value.
            scheduleRetry(replacing: nil)
        }
    }

    func closeCurrentConnection() async {
        let task = currentFlowTask
        let exchange = currentExchange
        let flow = currentFlow
        currentFlowTask = nil
        currentExchange = nil
        currentFlow = nil
        currentConnectionId = nil

        task?.cancel()
        await exchange?.close()
        await flow?.cancel()
        await task?.value
    }
}
