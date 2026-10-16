import Foundation
import os
import SubstrateSdk
import Starscream
import SDKLogger

public final class ChainSocket: ChainSocketProtocol, @unchecked Sendable {
    public struct Configuration: Sendable {
        public let connectionTimeout: TimeInterval
        public let pingInterval: TimeInterval
        public let pongTimeout: TimeInterval

        public init(connectionTimeout: TimeInterval, pingInterval: TimeInterval, pongTimeout: TimeInterval) {
            self.connectionTimeout = connectionTimeout
            self.pingInterval = pingInterval
            self.pongTimeout = pongTimeout
        }

        public static let `default` = Configuration(connectionTimeout: 10, pingInterval: 30, pongTimeout: 15)
    }

    fileprivate enum Phase {
        case idle
        case connecting
        case open
        case closed
    }

    fileprivate struct State {
        var phase: Phase = .idle
        var pending: [String] = []
        weak var delegate: ChainSocketDelegate?
        var pingTick: DispatchWorkItem?
        var pongDeadline: DispatchWorkItem?
        var connectDeadline: DispatchWorkItem?
    }

    public var delegate: ChainSocketDelegate? {
        get { state.withLockUnchecked { $0.delegate } }
        set { state.withLockUnchecked { $0.delegate = newValue } }
    }

    public let url: URL

    private let connection: WebSocketConnectionProtocol
    private let queue: DispatchQueue
    private let configuration: Configuration
    private let scheduler: ChainSocketScheduling
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    public init(
        url: URL,
        transportFactory: WebSocketConnectionFactoryProtocol,
        configuration: Configuration = .default,
        scheduler: ChainSocketScheduling = DispatchChainSocketScheduler(),
        logger: SDKLoggerProtocol
    ) {
        self.url = url
        self.configuration = configuration
        self.scheduler = scheduler
        self.logger = logger

        let queue = DispatchQueue(label: "io.polkadot.chain-socket.\(url.host ?? "unknown")")
        self.queue = queue

        connection = transportFactory.createConnection(
            for: url,
            processingQueue: queue,
            connectionTimeout: configuration.connectionTimeout
        )
        connection.delegate = self
    }

    deinit {
        let wasLive = state.withLockUnchecked { state -> Bool in
            state.cancelLiveness()
            return state.phase == .connecting || state.phase == .open
        }

        if wasLive {
            connection.forceDisconnect()
        }
    }

    public func open() {
        // Starscream's own timeout stops at TCP readiness; this one also bounds the WebSocket upgrade.
        let deadline = DispatchWorkItem { [weak self] in
            self?.shutDown(reason: .connectTimeout)
        }

        let shouldConnect = state.withLockUnchecked { state -> Bool in
            guard state.phase == .idle else { return false }
            state.phase = .connecting
            state.connectDeadline = deadline
            return true
        }

        guard shouldConnect else { return }
        connection.connect()
        scheduler.schedule(after: configuration.connectionTimeout, on: queue, execute: deadline)
    }

    public func send(text: String) {
        // Written under the lock so a send can't overtake the buffered frames being flushed on connect.
        let accepted = state.withLockUnchecked { [connection] state -> Bool in
            switch state.phase {
            case .connecting:
                state.pending.append(text)
                return true
            case .open:
                connection.write(string: text)
                return true
            case .idle,
                 .closed:
                return false
            }
        }

        if !accepted {
            logger.debug("chain socket \(host): frame dropped, socket is not open")
        }
    }

    public func close() {
        guard state.withLockUnchecked({ $0.markClosed() }) else { return }
        connection.forceDisconnect()
    }

    public func expire() {
        let work = DispatchWorkItem { [weak self] in
            self?.shutDown(reason: .resumed)
        }

        scheduler.async(on: queue, execute: work)
    }

    public func probe(timeout: TimeInterval) {
        let work = DispatchWorkItem { [weak self] in
            self?.sendPing(timeout: timeout)
        }

        scheduler.async(on: queue, execute: work)
    }

    private func shutDown(reason: ChainSocketCloseReason) {
        let (didClose, delegate) = state.withLockUnchecked { state in
            (state.markClosed(), state.delegate)
        }

        guard didClose else { return }

        connection.forceDisconnect()
        logger.debug("chain socket \(host): closed, reason \(reason)")
        delegate?.chainSocket(self, didCloseWith: reason)
    }

    func handle(_ event: WebSocketEvent) {
        switch event {
        case .connected:
            didConnect()
        case let .text(text):
            deliver(text)
        case let .binary(data):
            deliver(binary: data)
        case .pong:
            markAlive()
        case let .disconnected(reason, code):
            logger.debug("chain socket \(host): peer closed (\(code)) \(reason)")
            shutDown(reason: .peerClosed)
        case let .error(error):
            shutDown(reason: .transport(error))
        case .cancelled,
             .reconnectSuggested(true):
            shutDown(reason: .transport(nil))
        case .reconnectSuggested(false),
             .viabilityChanged,
             .ping:
            break
        }
    }
}

extension ChainSocket: WebSocketDelegate {
    public func didReceive(event: WebSocketEvent, client _: WebSocket) {
        handle(event)
    }
}

private extension ChainSocket {
    var host: String {
        url.host ?? "unknown"
    }

    func didConnect() {
        state.withLockUnchecked { [connection] state in
            guard state.phase == .connecting else { return }
            state.phase = .open
            state.cancelConnectDeadline()
            state.pending.forEach { connection.write(string: $0) }
            state.pending.removeAll()
            state.pingTick = makePingTick()
        }
    }

    func deliver(_ text: String) {
        let delegate = state.withLockUnchecked { state -> ChainSocketDelegate? in
            guard state.phase == .open else { return nil }
            state.cancelPongDeadline()
            return state.delegate
        }

        delegate?.chainSocket(self, didReceive: text)
    }

    func deliver(binary data: Data) {
        guard let text = String(data: data, encoding: .utf8) else {
            logger.warning("chain socket \(host): non-UTF-8 binary frame dropped")
            return
        }

        deliver(text)
    }

    func markAlive() {
        state.withLockUnchecked { $0.cancelPongDeadline() }
    }

    func makePingTick() -> DispatchWorkItem {
        let tick = DispatchWorkItem { [weak self] in
            self?.handlePingTick()
        }

        scheduler.schedule(after: configuration.pingInterval, on: queue, execute: tick)
        return tick
    }

    func handlePingTick() {
        let isOpen = state.withLockUnchecked { state -> Bool in
            guard state.phase == .open else { return false }
            state.pingTick = makePingTick()
            return true
        }

        guard isOpen else { return }

        sendPeriodicPing()
    }

    func sendPeriodicPing() {
        let isAwaitingPong = state.withLockUnchecked { $0.pongDeadline != nil }

        guard !isAwaitingPong else { return }

        sendPing(timeout: configuration.pongTimeout)
    }

    // Replaces any pending pong deadline, so a probe can tighten it.
    func sendPing(timeout: TimeInterval) {
        let deadline = DispatchWorkItem { [weak self] in
            self?.shutDown(reason: .livenessTimeout)
        }

        let shouldPing = state.withLockUnchecked { state -> Bool in
            guard state.phase == .open else { return false }
            state.cancelPongDeadline()
            state.pongDeadline = deadline
            return true
        }

        guard shouldPing else { return }

        connection.write(ping: Data(), completion: nil)
        scheduler.schedule(after: timeout, on: queue, execute: deadline)
    }
}

private extension ChainSocket.State {
    mutating func markClosed() -> Bool {
        guard phase != .closed else { return false }
        phase = .closed
        pending.removeAll()
        cancelLiveness()
        return true
    }

    mutating func cancelPongDeadline() {
        pongDeadline?.cancel()
        pongDeadline = nil
    }

    mutating func cancelConnectDeadline() {
        connectDeadline?.cancel()
        connectDeadline = nil
    }

    mutating func cancelLiveness() {
        pingTick?.cancel()
        pingTick = nil
        cancelPongDeadline()
        cancelConnectDeadline()
    }
}
