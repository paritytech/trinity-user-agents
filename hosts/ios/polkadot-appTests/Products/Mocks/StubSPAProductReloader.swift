import Foundation
@testable import polkadot_app

@MainActor
final class StubSPAProductReloader: SPAProductReloading {
    private(set) var serveCount = 0
    private var serveWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []

    func serveAppliedBuild() {
        serveCount += 1

        let ready = serveWaiters.filter { $0.count <= serveCount }
        serveWaiters.removeAll { $0.count <= serveCount }
        ready.forEach { $0.continuation.resume() }
    }

    func waitForServes(_ count: Int) async {
        guard serveCount < count else { return }

        await withCheckedContinuation { serveWaiters.append((count, $0)) }
    }
}
