import Foundation
import os
import ChainRegistry
import Foundation_iOS
import FoundationExt
import SDKLogger

/// One dedicated node socket per core `chain_connect`; socket deaths are reported, never hidden by reconnects.
///
/// Backgrounding alone never closes or probes a socket, so the pool itself gives products no reason to
/// retry against an app about to be suspended; real transport deaths are still reported whenever they
/// happen, including while the app keeps running in the background. On return to the foreground every
/// socket that lived through the background is revalidated: after a long absence all are expired,
/// after a short one each is probed and expired only if silent.
public final class TrUAPIV2SpecConnectionPool: TrUAPIChainConnecting, @unchecked Sendable {
    public struct ResumePolicy: Sendable {
        /// Background time after which sockets are expired instead of probed.
        public let expiryThreshold: TimeInterval
        public let probeTimeout: TimeInterval

        public init(expiryThreshold: TimeInterval, probeTimeout: TimeInterval) {
            self.expiryThreshold = expiryThreshold
            self.probeTimeout = probeTimeout
        }

        public static let `default` = ResumePolicy(expiryThreshold: 30, probeTimeout: 5)
    }

    fileprivate struct State {
        weak var eventHandler: TrUAPIChainEventHandling?
        var sockets: [UInt32: ChainSocketProtocol] = [:]
        var connectionIds: [ObjectIdentifier: UInt32] = [:]
        var nextConnectionId: UInt32 = 1
        /// Grows per chain so a reconnect after a dead node lands on the next one.
        var nodeAttempts: [ChainModel.Id: Int] = [:]
        var backgroundedAt: Date?
        /// Tail of the serial chain that keeps background and foreground handling in order.
        var lifecycleTask: Task<Void, Never>?
    }

    public var eventHandler: TrUAPIChainEventHandling? {
        get { state.withLockUnchecked { $0.eventHandler } }
        set { state.withLockUnchecked { $0.eventHandler = newValue } }
    }

    private let chainRegistry: ChainRegistryProtocol
    private let applicationHandler: ApplicationHandlerProtocol
    private let dateProvider: DateProviding
    private let resumePolicy: ResumePolicy
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    public init(
        chainRegistry: ChainRegistryProtocol,
        applicationHandler: ApplicationHandlerProtocol,
        dateProvider: DateProviding,
        resumePolicy: ResumePolicy = .default,
        logger: SDKLoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.applicationHandler = applicationHandler
        self.dateProvider = dateProvider
        self.resumePolicy = resumePolicy
        self.logger = logger

        applicationHandler.delegate = self
    }

    public func connect(genesisHash: Data) -> UInt32? {
        guard let socket = makeSocket(for: genesisHash) else {
            logger.debug("TrUAPI chain \(genesisHash.toHex()) has no host-managed connection; unsupported")
            return nil
        }

        socket.delegate = self

        // Registered before open so a close fired during open resolves to this id.
        let connectionId = state.withLockUnchecked { state in
            let connectionId = state.nextConnectionId
            state.nextConnectionId += 1
            state.sockets[connectionId] = socket
            state.connectionIds[ObjectIdentifier(socket)] = connectionId
            return connectionId
        }

        socket.open()

        return connectionId
    }

    public func send(connectionId: UInt32, request: String) throws {
        guard let socket = state.withLockUnchecked({ $0.sockets[connectionId] }) else {
            throw TrUAPIChainConnectionError.unknownConnection(connectionId)
        }

        socket.send(text: request)
    }

    public func close(connectionId: UInt32) {
        let socket = state.withLockUnchecked { $0.removeSocket(for: connectionId) }
        socket?.close()
    }

    public func closeAll() {
        let sockets = state.withLockUnchecked { state -> [ChainSocketProtocol] in
            let sockets = Array(state.sockets.values)
            state.sockets.removeAll()
            state.connectionIds.removeAll()
            return sockets
        }

        sockets.forEach { $0.close() }
    }
}

extension TrUAPIV2SpecConnectionPool: ChainSocketDelegate {
    public func chainSocket(_ socket: ChainSocketProtocol, didReceive text: String) {
        let (connectionId, handler) = state.withLockUnchecked { state in
            (state.connectionIds[ObjectIdentifier(socket)], state.eventHandler)
        }

        guard let connectionId else { return }
        handler?.chainDidReceiveResponse(connectionId: connectionId, json: text)
    }

    public func chainSocket(_ socket: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason) {
        let (connectionId, handler) = state.withLockUnchecked { state -> (UInt32?, TrUAPIChainEventHandling?) in
            guard let connectionId = state.connectionIds[ObjectIdentifier(socket)] else {
                return (nil, nil)
            }

            _ = state.removeSocket(for: connectionId)
            return (connectionId, state.eventHandler)
        }

        guard let connectionId else { return }

        logger.debug("TrUAPI chain connection \(connectionId) closed: \(reason)")
        handler?.chainDidClose(connectionId: connectionId)
    }
}

extension TrUAPIV2SpecConnectionPool: ApplicationHandlerDelegate {
    public func didReceiveDidEnterBackground(notification _: Notification) {
        enqueueLifecycleEvent { pool in
            await pool.markBackgrounded()
        }
    }

    public func didReceiveWillEnterForeground(notification _: Notification) {
        enqueueLifecycleEvent { pool in
            await pool.revalidateAfterBackground()
        }
    }
}

extension TrUAPIV2SpecConnectionPool {
    /// Waits until every background/foreground notification received so far has been handled.
    func awaitLifecycleEvents() async {
        await state.withLockUnchecked { $0.lifecycleTask }?.value
    }
}

private extension TrUAPIV2SpecConnectionPool {
    func makeSocket(for genesisHash: Data) -> ChainSocketProtocol? {
        guard let chain = chainRegistry.getChainByGenesis(for: genesisHash.toHex()) else { return nil }

        let attempt = state.withLockUnchecked { $0.takeNodeAttempt(for: chain.chainId) }
        return chainRegistry.makeChainSocket(for: chain.chainId, attempt: attempt)
    }

    // Date reads are async, so each event runs after the previous one to keep background before foreground.
    func enqueueLifecycleEvent(_ handle: @escaping @Sendable (TrUAPIV2SpecConnectionPool) async -> Void) {
        state.withLockUnchecked { state in
            let previous = state.lifecycleTask
            state.lifecycleTask = Task { [weak self] in
                await previous?.value
                guard let self else { return }
                await handle(self)
            }
        }
    }

    func markBackgrounded() async {
        let enteredAt = await dateProvider.read()

        state.withLockUnchecked { state in
            state.backgroundedAt = state.backgroundedAt ?? enteredAt
        }
    }

    func revalidateAfterBackground() async {
        guard let backgroundedAt = state.withLockUnchecked({ $0.backgroundedAt }) else { return }

        let duration = await dateProvider.read().timeIntervalSince(backgroundedAt)

        let sockets = state.withLockUnchecked { state in
            state.backgroundedAt = nil
            return Array(state.sockets.values)
        }

        revalidate(sockets, afterBackground: duration)
    }

    // Socket callbacks re-enter the pool lock, so sockets are touched only outside it.
    func revalidate(_ sockets: [ChainSocketProtocol], afterBackground duration: TimeInterval) {
        guard !sockets.isEmpty else { return }

        if duration >= resumePolicy.expiryThreshold {
            logger.debug("TrUAPI expiring \(sockets.count) chain sockets after \(Int(duration))s in background")
            sockets.forEach { $0.expire() }
        } else {
            logger.debug("TrUAPI probing \(sockets.count) chain sockets after \(Int(duration))s in background")
            sockets.forEach { $0.probe(timeout: resumePolicy.probeTimeout) }
        }
    }
}

private extension TrUAPIV2SpecConnectionPool.State {
    mutating func takeNodeAttempt(for chainId: ChainModel.Id) -> Int {
        let attempt = nodeAttempts[chainId, default: 0]
        nodeAttempts[chainId] = attempt + 1
        return attempt
    }

    mutating func removeSocket(for connectionId: UInt32) -> ChainSocketProtocol? {
        guard let socket = sockets.removeValue(forKey: connectionId) else { return nil }
        connectionIds.removeValue(forKey: ObjectIdentifier(socket))
        return socket
    }
}
