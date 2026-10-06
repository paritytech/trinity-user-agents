import Foundation
import MessageExchangeKit
import SubstrateSdk
import Testing
@testable import polkadot_app

struct SSOTruAPICoordinatorTests {
    @Test func cancelledSetupCannotReplaceTheRestartedSender() async throws {
        let state = SSOTruAPICoordinator.State()
        let sender = PolkadotHostMessageSender<SSORawHostMessage>()
        let stale = RecordingRawExchangeService(sender: sender)
        let current = RecordingRawExchangeService(sender: sender)
        let entered = AsyncStream<CheckedContinuation<Void, Never>>.makeStream()
        let finished = AsyncStream<Void>.makeStream()
        let installed = AsyncStream<Void>.makeStream()
        await state.start {
            await withCheckedContinuation { entered.continuation.yield($0) }
            await sender.setExchangeService(AnyMessageExchangeService(stale))
            await state.finished()
            finished.continuation.yield(())
        }
        var enteredIterator = entered.stream.makeAsyncIterator()
        let release = try #require(await enteredIterator.next())
        await state.reset()
        await state.start {
            await sender.setExchangeService(AnyMessageExchangeService(current))
            installed.continuation.yield(())
        }
        var installedIterator = installed.stream.makeAsyncIterator()
        _ = await installedIterator.next()
        release.resume()
        var finishedIterator = finished.stream.makeAsyncIterator()
        _ = await finishedIterator.next()

        let encoder = ScaleEncoder()
        try "request".encode(scaleEncoder: encoder)
        var bytes = encoder.encode()
        bytes.append(0xAA)
        let message = try SSORawHostMessage(rawBytes: bytes)
        try await sender.postMessage(message, to: PolkadotSignInHost(
            accountId: Data(repeating: 1, count: 32),
            publicKey: Data(repeating: 2, count: 32),
            name: "Host",
            iconUrl: nil
        ))
        #expect([stale.messages, current.messages] == [[], [message]])
        await state.reset()
    }
}

private final class RecordingRawExchangeService: MessageExchangeServicing, @unchecked Sendable {
    typealias Message = OpaqueSSORawHostMessage
    private weak var sender: PolkadotHostMessageSender<SSORawHostMessage>?
    private let lock = NSLock()
    private var queued: [SSORawHostMessage] = []
    var messages: [SSORawHostMessage] { lock.withLock { queued } }

    init(sender: PolkadotHostMessageSender<SSORawHostMessage>) { self.sender = sender }
    func updateSessions(_: Set<MessageExchange.SessionRequest>) {}

    func addMessagesToQueue(_ messages: [OpaqueSSORawHostMessage], for _: MessageExchange.Peer) {
        let raw = messages.map(\.message)
        lock.withLock { queued.append(contentsOf: raw) }
        Task { await sender?.handleDidPostMessages(raw, withError: nil) }
    }
}
