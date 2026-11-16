import Foundation
import FoundationExt

@MainActor
final class ApplicationActivityStub: ApplicationActivityProviding {
    private(set) var isInBackground: Bool
    private(set) var waitCallCount = 0
    private(set) var cancelledWaitCount = 0

    let changes = StateChangeSignal()

    private var waiters: [UUID: CheckedContinuation<Void, Never>] = [:]

    init(isInBackground: Bool = false) {
        self.isInBackground = isInBackground
    }

    func waitUntilInForeground() async {
        guard isInBackground else { return }

        waitCallCount += 1
        let waiterId = UUID()

        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                waiters[waiterId] = continuation
                changes.signal()
            }
        } onCancel: {
            Task { @MainActor in self.cancel(waiterId) }
        }
    }

    func enterForeground() {
        isInBackground = false

        let pending = waiters
        waiters = [:]
        pending.values.forEach { $0.resume() }
    }
}

private extension ApplicationActivityStub {
    func cancel(_ waiterId: UUID) {
        guard let waiter = waiters.removeValue(forKey: waiterId) else { return }

        cancelledWaitCount += 1
        waiter.resume()
        changes.signal()
    }
}
