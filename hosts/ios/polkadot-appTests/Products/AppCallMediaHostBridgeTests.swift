import AsyncExtensions
import Foundation
import os
import Testing
import TrUAPIHost
import WebRTC
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

    /// The outgoing call screen opens at once under a call id that the host chose, and the product gets the call
    /// under that id.
    @Test func aCallOfTheUserOpensTheOutgoingCallScreenAtOnce() async throws {
        let published = RecordingPublisher()
        let presented = OSAllocatedUnfairLock<[CallRole]>(initialState: [])
        let bridge = makeBridge(publisher: published) { role in presented.withLock { $0.append(role) } }

        await bridge.startOutgoingCall(productId: "chat.dot", roomId: "dm-7", video: true)

        guard case let .callRequested(video, callId) = try #require(published.events.first?.event) else {
            Issue.record("expected a call request, got \(published.events)")
            return
        }
        #expect(video)
        #expect(UUID(uuidString: callId) != nil)
        #expect(presented.withLock { $0 } == [.initiator])

        // The product starts the media of the call that runs on the screen already.
        try await bridge.startCallMedia(
            productId: "chat.dot",
            roomId: "dm-7",
            callId: callId,
            video: true,
            offerer: true
        )
        #expect(presented.withLock { $0 } == [.initiator])
    }

    /// "Ringing" shows once a device of the contact read the offer message, whose id is the call id.
    @Test func theOfferWatcherTellsOnceWhenTheOfferIsRead() async {
        let reads = OSAllocatedUnfairLock(initialState: 0)
        let watcher = ProductCallOfferWatcher(offerId: "c1") { reads.withLock { $0 += 1 } }

        await watcher.apply(roomId: "dm-7", update: .statusChanged(messageId: "c1", status: .sent))
        await watcher.apply(roomId: "dm-7", update: .statusChanged(messageId: "other", status: .read))
        #expect(reads.withLock { $0 } == 0)

        await watcher.apply(roomId: "dm-7", update: .statusChanged(messageId: "c1", status: .read))
        await watcher.apply(roomId: "dm-7", update: .statusChanged(messageId: "c1", status: .read))
        #expect(reads.withLock { $0 } == 1)
    }

    /// A second offer while a call runs is turned away, as the native chat turns it away.
    @Test func anOfferDuringACallIsTurnedAway() async throws {
        let published = RecordingPublisher()
        let store = ChatRoomCallStore()
        let bridge = makeBridge(publisher: published) { _ in }
        bridge.followRinging(of: store) { true }

        await bridge.startOutgoingCall(productId: "chat.dot", roomId: "dm-7", video: false)
        try await Task.sleep(for: .milliseconds(20))
        store.set(
            .ringing(callId: "c2", video: false, caller: "contact"),
            for: .chatExtension("chat.dot", roomId: "dm-8")
        )

        for _ in 0 ..< 200 where published.events.count < 2 {
            try await Task.sleep(for: .milliseconds(2))
        }
        #expect(published.events.last?.event == .callAnswered(callId: "c2", accept: false))
        #expect(published.events.last?.roomId == "dm-8")
    }

    /// A declined call that the product ends leaves no call behind, so the next offer rings.
    @Test func aDeclinedCallEndsWhenTheProductEndsIt() async {
        let calls = ProductCalls()
        let key = ProductCalls.Key(productId: "chat.dot", roomId: "dm-7", callId: "c1")
        let engine = ProductRingingCallEngine(inner: StubCallEngine()) { _ in }
        _ = await calls.start(key, signaler: ProductCallSignaler(callId: "c1") { _ in }, ringing: engine)

        await engine.endCall(notifiesRemote: true)
        await calls.stopRinging(key)

        #expect(await !calls.isRunning(key))
        #expect(await !calls.hasCall(otherThan: ProductCalls.Key(productId: "chat.dot", roomId: "dm-8", callId: "c2")))
    }

    private func makeBridge(
        publisher: RecordingPublisher,
        presented: @escaping @Sendable (CallRole) -> Void
    ) -> AppCallMediaHostBridge {
        AppCallMediaHostBridge(
            publisher: publisher,
            feedListeners: ProductFeedListeners(),
            makeEngine: { _, _, _ in StubCallEngine() },
            present: { _, _, role, _, _ in presented(role) },
            peer: { _, _ in CallPeer(name: "alice", accountId: Data(count: 32)) }
        )
    }
}

private final class RecordingPublisher: ChatRoomEventPublishing, @unchecked Sendable {
    struct Published: Equatable {
        let event: Chat.RoomEvent
        let roomId: String
    }

    private let lock = NSLock()
    private var recorded: [Published] = []

    var events: [Published] {
        lock.withLock { recorded }
    }

    func publish(_ event: Chat.RoomEvent, productId _: ChatExtension.Id, roomId: String) throws {
        lock.withLock { recorded.append(Published(event: event, roomId: roomId)) }
    }
}

private final class StubCallEngine: CallEngineProtocol, @unchecked Sendable {
    func observeState() -> AnyAsyncSequence<CallEngineState> {
        AsyncEmptySequence().eraseToAnyAsyncSequence()
    }

    func observeRemoteMediaState() -> AnyAsyncSequence<CallRemoteMediaState> {
        AsyncEmptySequence().eraseToAnyAsyncSequence()
    }

    func connect() {}

    func endCall(notifiesRemote _: Bool) async {}

    func attach(localRenderer _: RTCVideoRenderer) {}

    func attach(remoteRenderer _: RTCVideoRenderer) {}

    func observeVideoState() -> AnyAsyncSequence<Bool> {
        AsyncEmptySequence().eraseToAnyAsyncSequence()
    }

    func observeVideoCaptureFailure() -> AnyAsyncSequence<Void> {
        AsyncEmptySequence().eraseToAnyAsyncSequence()
    }

    var isMuted: Bool {
        get async { false }
    }

    func setMuted(_: Bool) async -> Bool {
        false
    }

    var isVideoEnabled: Bool {
        get async { false }
    }

    func setVideoEnabled(_: Bool) async {}
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
