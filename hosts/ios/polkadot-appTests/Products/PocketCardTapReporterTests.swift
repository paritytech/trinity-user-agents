import Foundation
import IssueMonitoring
import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
struct PocketCardTapReporterTests {
    /// Timed to the frame drawn after the held page is shown, not to the moment it was put on screen.
    @Test func aReusedPageIsReportedOnceItsFrameIsDrawn() throws {
        let recorder = RecordingHostEventRecorder()
        let frames = PendingFrames()
        let clock = SteppingClock(start: 1_000, step: 300)
        let reporter = PocketCardTapReporter(
            telemetry: HostLoadTelemetry(recorder: recorder, monotonicMilliseconds: { clock.read() }),
            nextFrame: frames.wait
        )
        let session = UUID()
        let tap = HostOpenTap(
            hostSessionID: UUID(),
            source: .pocketCard,
            openedAtEpochMilliseconds: 0,
            monotonicMilliseconds: 900
        )

        try reporter.reportReused(tap, productId: ProductId("game.paseo"), hostSessionId: session)
        #expect(recorder.events.isEmpty)

        frames.draw()

        let event = try #require(recorder.events.first)
        #expect(recorder.events.count == 1)
        #expect(event.name == "host.pocket.card_tapped")
        #expect(event.tags["outcome"] == "reused")
        #expect(event.tags["open.source"] == "pocket_card")
        #expect(event.tags["host.session_id"] == session.uuidString.lowercased())
        #expect(event.extras["reuse_ms"] == .integer(100))
        #expect(event.extras["duration_ms"] == .integer(100))
    }

    @Test func noTapWaitsForNoFrame() throws {
        let frames = PendingFrames()
        let reporter = PocketCardTapReporter(
            telemetry: HostLoadTelemetry(recorder: RecordingHostEventRecorder()),
            nextFrame: frames.wait
        )

        try reporter.reportReused(nil, productId: ProductId("game.paseo"), hostSessionId: nil)

        #expect(frames.waiting == 0)
    }

    @Test func theNextDrawnFrameArrives() async {
        await withCheckedContinuation { continuation in
            NextDrawnFrame.wait { continuation.resume() }
        }
    }
}

@MainActor
private final class PendingFrames {
    private var callbacks: [@MainActor () -> Void] = []

    var waiting: Int { callbacks.count }

    func wait(_ drawn: @escaping @MainActor () -> Void) {
        callbacks.append(drawn)
    }

    func draw() {
        let drawn = callbacks
        callbacks = []
        drawn.forEach { $0() }
    }
}
