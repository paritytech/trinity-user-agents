import Foundation
import os
import SubstrateSdk
import Starscream

final class RecordingWebSocketConnection: WebSocketConnectionProtocol, @unchecked Sendable {
    private struct Calls {
        var connectCount = 0
        var forceDisconnectCount = 0
        var pingCount = 0
        var written: [String] = []
    }

    let callbackQueue: DispatchQueue
    weak var delegate: WebSocketDelegate?

    /// Runs after each ping is recorded, so a test can answer it.
    var onPing: (() -> Void)?

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
        onPing?()
    }

    func write(pong _: Data, completion _: (() -> Void)?) {}
}
