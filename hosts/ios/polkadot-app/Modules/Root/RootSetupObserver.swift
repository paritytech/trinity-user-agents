import Foundation
import os

final class RootSetupObserver {
    private struct PathState {
        var isSatisfied = true
        var isAwaitingRecovery = false
    }

    private enum PathTransition {
        case recovered
        case dropped
    }

    let signals: AsyncStream<RootSetupSignal>

    private let pathMonitor: NetworkPathMonitoring
    private let pathState = OSAllocatedUnfairLock(initialState: PathState())
    private let continuation: AsyncStream<RootSetupSignal>.Continuation
    private var pathTask: Task<Void, Never>?

    var isPathSatisfied: Bool {
        pathState.withLock { $0.isSatisfied }
    }

    init(pathMonitor: NetworkPathMonitoring) {
        self.pathMonitor = pathMonitor
        (signals, continuation) = AsyncStream.makeStream(of: RootSetupSignal.self)
    }

    deinit {
        stop()
    }

    func start() {
        guard pathTask == nil else {
            return
        }

        let stream = pathMonitor.pathStream()

        pathTask = Task { [weak self] in
            do {
                var isFirstValue = true
                for try await isAvailable in stream {
                    guard let self else { return }

                    let transition = consumeTransition(isAvailable: isAvailable)

                    // The monitor replays the current path as its first value: it establishes the
                    // baseline the later values are compared against rather than describing a change.
                    // Reporting it would fail an already offline launch outright, and a warm one still
                    // reaches a destination with the path down; the offline deadline bounds a cold one.
                    guard !isFirstValue else {
                        isFirstValue = false
                        continue
                    }

                    guard let transition else { continue }

                    switch transition {
                    case .recovered:
                        continuation.yield(.connectivityRecovered)
                    case .dropped:
                        continuation.yield(.connectivityLost)
                    }
                }
            } catch {}
        }
    }

    func stop() {
        pathTask?.cancel()
        continuation.finish()
    }

    func claimConnectivityFailure() -> Bool {
        pathState.withLock { state in
            guard !state.isSatisfied else { return false }
            state.isAwaitingRecovery = true
            return true
        }
    }
}

private extension RootSetupObserver {
    /// Records the new satisfaction and classifies the transition: recovered when availability returns
    /// while awaiting retry, dropped when availability is lost.
    private func consumeTransition(isAvailable: Bool) -> PathTransition? {
        pathState.withLock { state in
            let wasAvailable = state.isSatisfied
            state.isSatisfied = isAvailable

            if isAvailable, !wasAvailable, state.isAwaitingRecovery {
                state.isAwaitingRecovery = false
                return .recovered
            }

            if !isAvailable, wasAvailable {
                return .dropped
            }

            return nil
        }
    }
}
