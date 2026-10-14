@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockDeviceSyncMessageTransport: DeviceSyncMessageTransporting, @unchecked Sendable {
    private let lock = NSLock()
    private let sentMessageEvents = DeviceSyncTestEventRecorder<Data>()
    private let closeEvents = DeviceSyncTestEventRecorder<Void>()
    private var _didClose = false
    private var _sentMessages = [Data]()
    private var _sentMessageBatches = [[Data]]()

    var didClose: Bool {
        lock.withLock { _didClose }
    }

    var sentMessages: [Data] {
        lock.withLock { _sentMessages }
    }

    var sentMessageBatches: [[Data]] {
        lock.withLock { _sentMessageBatches }
    }

    func open(delegate _: AnyPeerSessionDelegate<Data>) async throws {}

    func close() async {
        lock.withLock { _didClose = true }
        closeEvents.record(())
    }

    func send(_ messages: [Data]) async {
        lock.withLock {
            _sentMessages.append(contentsOf: messages)
            _sentMessageBatches.append(messages)
        }
        messages.forEach(sentMessageEvents.record)
    }

    func waitForSentMessageCount(_ count: Int) async -> [Data] {
        await sentMessageEvents.waitForCount(count)
    }

    func waitUntilClosed() async {
        _ = await closeEvents.waitForCount(1)
    }
}
