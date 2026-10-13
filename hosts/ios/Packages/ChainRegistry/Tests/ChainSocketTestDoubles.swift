import Foundation
import os
import SubstrateSdk
import Starscream
import SDKLogger
import ChainRegistry

final class RecordingWebSocketConnection: WebSocketConnectionProtocol, @unchecked Sendable {
    private struct Calls {
        var connectCount = 0
        var forceDisconnectCount = 0
        var pingCount = 0
        var written: [String] = []
    }

    let callbackQueue: DispatchQueue
    weak var delegate: WebSocketDelegate?

    private let calls = OSAllocatedUnfairLock(initialState: Calls())

    var connectCount: Int { calls.withLock { $0.connectCount } }
    var forceDisconnectCount: Int { calls.withLock { $0.forceDisconnectCount } }
    var pingCount: Int { calls.withLock { $0.pingCount } }
    var written: [String] { calls.withLock { $0.written } }

    init(callbackQueue: DispatchQueue) {
        self.callbackQueue = callbackQueue
    }

    func connect() {
        calls.withLock { $0.connectCount += 1 }
    }

    func disconnect(closeCode _: UInt16) {}

    func forceDisconnect() {
        calls.withLock { $0.forceDisconnectCount += 1 }
    }

    func write(string: String, completion _: (() -> Void)?) {
        calls.withLock { $0.written.append(string) }
    }

    func write(stringData _: Data, completion _: (() -> Void)?) {}
    func write(data _: Data, completion _: (() -> Void)?) {}

    func write(ping _: Data, completion _: (() -> Void)?) {
        calls.withLock { $0.pingCount += 1 }
    }

    func write(pong _: Data, completion _: (() -> Void)?) {}
}

final class RecordingTransportFactory: WebSocketConnectionFactoryProtocol, @unchecked Sendable {
    private let connections = OSAllocatedUnfairLock<[(url: URL, connection: RecordingWebSocketConnection)]>(
        uncheckedState: []
    )

    var created: [(url: URL, connection: RecordingWebSocketConnection)] {
        connections.withLockUnchecked { $0 }
    }

    func createConnection(
        for url: URL,
        processingQueue: DispatchQueue,
        connectionTimeout _: TimeInterval
    ) -> WebSocketConnectionProtocol {
        let connection = RecordingWebSocketConnection(callbackQueue: processingQueue)
        connections.withLockUnchecked { $0.append((url, connection)) }
        return connection
    }
}

final class RecordingChainSocketDelegate: ChainSocketDelegate, @unchecked Sendable {
    private struct Events {
        var received: [String] = []
        var closeReasons: [ChainSocketCloseReason] = []
    }

    private let events = OSAllocatedUnfairLock(uncheckedState: Events())

    var onClose: ((ChainSocketCloseReason) -> Void)?

    var received: [String] { events.withLockUnchecked { $0.received } }
    var closeReasons: [ChainSocketCloseReason] { events.withLockUnchecked { $0.closeReasons } }

    func chainSocket(_: ChainSocketProtocol, didReceive text: String) {
        events.withLockUnchecked { $0.received.append(text) }
    }

    func chainSocket(_: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason) {
        events.withLockUnchecked { $0.closeReasons.append(reason) }
        onClose?(reason)
    }
}

extension ChainSocketCloseReason {
    var isPeerClosed: Bool {
        if case .peerClosed = self { return true }
        return false
    }

    var isTransport: Bool {
        if case .transport = self { return true }
        return false
    }

    var isLivenessTimeout: Bool {
        if case .livenessTimeout = self { return true }
        return false
    }

    var isSleep: Bool {
        if case .sleep = self { return true }
        return false
    }
}

struct SilentLogger: SDKLoggerProtocol {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}
