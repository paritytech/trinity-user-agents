import Foundation

/// A clock and the date provider judged against it, paired so a test drives both from virtual time.
public struct TimeSource: Sendable {
    public let clock: any Clock<Duration>
    public let dateProvider: any DateProviding

    public init(clock: any Clock<Duration>, dateProvider: any DateProviding) {
        self.clock = clock
        self.dateProvider = dateProvider
    }

    public static let live = TimeSource(clock: ContinuousClock(), dateProvider: NowDateProvider())
}

public extension TimeSource {
    /// A deadline already passed fires at once. The date is re-read after every sleep, so a clock running
    /// ahead of the date provider cannot fire early.
    func fires(at deadline: Date) -> AsyncStream<Void> {
        AsyncStream { continuation in
            let task = Task {
                do {
                    var now = await dateProvider.read()
                    while now < deadline {
                        try await clock.sleep(for: .seconds(deadline.timeIntervalSince(now)))
                        now = await dateProvider.read()
                    }
                } catch {
                    return
                }
                continuation.yield()
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
