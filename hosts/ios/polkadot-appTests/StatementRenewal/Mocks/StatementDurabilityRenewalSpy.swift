import Foundation

@testable import polkadot_app

final class StatementDurabilityRenewalSpy: StatementDurabilityRenewing, @unchecked Sendable {
    enum Event: Equatable {
        case started
        case cancelled
    }

    let events: AsyncStream<Event>

    private let eventSink: AsyncStream<Event>.Continuation
    private let holdsUntilCancelled: Bool

    init(holdsUntilCancelled: Bool = false) {
        (events, eventSink) = AsyncStream.makeStream(of: Event.self)
        self.holdsUntilCancelled = holdsUntilCancelled
    }

    func renewAll() async -> StatementDurabilityRenewalReport {
        eventSink.yield(.started)

        if holdsUntilCancelled {
            let (cancellation, cancelled) = AsyncStream.makeStream(of: Void.self)
            await withTaskCancellationHandler {
                for await _ in cancellation {}
            } onCancel: {
                cancelled.finish()
            }
            eventSink.yield(.cancelled)
        }

        return .cancelled
    }
}
