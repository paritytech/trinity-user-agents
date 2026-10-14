import Foundation
import os

final class SearchRunner {
    enum State<SearchResult> {
        case started
        case waiting
        case result(SearchResult)
    }

    private enum Constants {
        static let debounceDelay: Duration = .milliseconds(300)
        static let waitingDelay: Duration = .milliseconds(500)
        static let minLoaderDuration: Duration = .milliseconds(500)
    }

    private let clock: any Clock<Duration>

    init(clock: any Clock<Duration> = ContinuousClock()) {
        self.clock = clock
    }

    /// A failure from `operation()` ends the stream, so callers map their own errors into elements.
    func run<Source: AsyncSequence>(
        _ operation: @escaping () -> Source,
        hasContent: @escaping @Sendable (Source.Element) -> Bool
    ) -> AsyncStream<State<Source.Element>> {
        AsyncStream { continuation in
            let clock = clock
            let loaderState = SearchLoaderState()
            let loaderTask = makeLoaderTask(loaderState: loaderState) { continuation.yield(.waiting) }

            let searchTask = Task {
                continuation.yield(.started)

                try? await clock.sleep(for: Constants.debounceDelay)
                guard !Task.isCancelled else {
                    continuation.finish()
                    return
                }

                do {
                    for try await element in operation() {
                        // Only an empty phase waits out the loader floor, content replaces the loader at once.
                        if !hasContent(element), loaderState.isLoaderShown {
                            await loaderTask.value
                        }

                        guard !Task.isCancelled else { break }

                        continuation.yield(.result(element))
                    }
                } catch {
                    // Sequence ended
                }

                loaderState.complete()
                loaderTask.cancel()
                continuation.finish()
            }

            continuation.onTermination = { _ in
                loaderTask.cancel()
                searchTask.cancel()
            }
        }
    }
}

private extension SearchRunner {
    /// Calls `onShow` once the loader is due and then keeps running for the minimum loader
    /// duration, so that awaiting this task waits out the floor.
    func makeLoaderTask(
        loaderState: SearchLoaderState,
        onShow: @escaping @Sendable () -> Void
    ) -> Task<Void, Never> {
        let clock = clock

        return Task {
            try? await clock.sleep(for: Constants.debounceDelay + Constants.waitingDelay)

            // A loader that would appear after the search completed is dropped.
            guard !Task.isCancelled, loaderState.showLoader() else { return }

            onShow()

            try? await clock.sleep(for: Constants.minLoaderDuration)
        }
    }
}

private final class SearchLoaderState: Sendable {
    private struct State {
        var isLoaderShown = false
        var isCompleted = false
    }

    private let stateLock = OSAllocatedUnfairLock(initialState: State())

    var isLoaderShown: Bool {
        stateLock.withLock { $0.isLoaderShown }
    }

    /// Marks the loader as shown unless the search already completed, in which case it is dropped.
    func showLoader() -> Bool {
        stateLock.withLock { state in
            guard !state.isCompleted else { return false }

            state.isLoaderShown = true

            return true
        }
    }

    func complete() {
        stateLock.withLock { $0.isCompleted = true }
    }
}
