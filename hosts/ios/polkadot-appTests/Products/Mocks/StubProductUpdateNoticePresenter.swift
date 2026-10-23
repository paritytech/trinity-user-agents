import Foundation
@testable import polkadot_app

@MainActor
final class StubProductUpdateNoticePresenter: ProductUpdateNoticePresenting {
    private(set) var shownProductNames: [String] = []
    private(set) var isShowing = false
    private var onUpdate: (() -> Void)?
    private var onDismiss: (() -> Void)?
    private var showWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []

    func show(productName: String, onUpdate: @escaping () -> Void, onDismiss: @escaping () -> Void) {
        shownProductNames.append(productName)
        isShowing = true
        self.onUpdate = onUpdate
        self.onDismiss = onDismiss

        let count = shownProductNames.count
        let ready = showWaiters.filter { $0.count <= count }
        showWaiters.removeAll { $0.count <= count }
        ready.forEach { $0.continuation.resume() }
    }

    func hide() {
        isShowing = false
    }

    func waitForShows(_ count: Int) async {
        guard shownProductNames.count < count else { return }

        await withCheckedContinuation { showWaiters.append((count, $0)) }
    }

    func simulateUpdate() {
        onUpdate?()
    }

    func simulateDismiss() {
        onDismiss?()
    }
}
