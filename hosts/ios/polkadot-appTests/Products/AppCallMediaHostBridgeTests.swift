import Foundation
import Testing
@testable import polkadot_app

/// The signals of the call of a product room cross the product as opaque bytes, in both directions, and the call
/// screen of the host ends the call for both sides through the product.
struct AppCallMediaHostBridgeTests {
    @Test func theSignalsOfOneSideReachTheEngineOfTheOtherSide() async throws {
        let sent = RecordingEvents()
        let caller = ProductCallSignaler(callId: "c1") { sent.add($0) }
        let callee = ProductCallSignaler(callId: "c1") { _ in }
        let candidate = PeerConnectionCandidate(sdp: "candidate:1", sdpMLineIndex: 0, sdpMid: "0")

        let result = try await caller.send([.offer("v=0"), .candidates([candidate])])
        #expect(result.isFullySent)
        guard case let .callSignal(callId, signal) = try #require(sent.events.first) else {
            Issue.record("expected a call signal, got \(sent.events)")
            return
        }
        #expect(callId == "c1")

        callee.receive(signal)
        var received: [PeerConnectionSignal] = []
        for try await signal in callee.signals {
            received.append(signal)
            if received.count == 2 { break }
        }
        #expect(received == [.offer("v=0"), .candidates([candidate])])
    }

    @Test func endingOnTheCallScreenEndsTheCallOfTheRoom() async throws {
        let sent = RecordingEvents()
        let signaler = ProductCallSignaler(callId: "c1") { sent.add($0) }

        try await signaler.send([.closed])

        #expect(sent.events.count == 2)
        #expect(sent.events.last == .callEnded(callId: "c1"))
    }

    @Test func aSignalBeforeItsCallStartsWaitsForTheStart() async {
        let calls = ProductCalls()
        let key = ProductCalls.Key(productId: "verbosity.dot", roomId: "dm-7", callId: "c1")
        let signaler = ProductCallSignaler(callId: "c1") { _ in }

        #expect(await calls.signaler(of: key, keeping: Data([1])) == nil)
        #expect(await calls.signaler(of: key, keeping: Data([2])) == nil)
        #expect(await calls.start(key, signaler: signaler) == [Data([1]), Data([2])])
        #expect(await calls.signaler(of: key, keeping: Data([3])) === signaler)
        #expect(await calls.stop(key) === signaler)
        #expect(await !calls.isRunning(key))
    }
}

private final class RecordingEvents: @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [Chat.RoomEvent] = []

    var events: [Chat.RoomEvent] {
        lock.withLock { recorded }
    }

    func add(_ event: Chat.RoomEvent) {
        lock.withLock { recorded.append(event) }
    }
}
