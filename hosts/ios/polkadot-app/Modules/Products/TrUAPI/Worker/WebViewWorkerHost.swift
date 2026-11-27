import Foundation
import os
import TrUAPIHost

/// Owns the platform views opened for the core's startup attempts.
///
/// The core calls `start` and `stop` on whichever thread changed worker demand or reported a
/// failure, so a `failed` report made here can run both before it returns, and none is made
/// with the lock held. `stop` arrives once for each started attempt, after its `start` returned.
/// The core ignores what a stopped or replaced attempt reports, so a startup that `stop`
/// cancelled reports no failure. The startup task is created with the lock held because it takes
/// the same lock to store its view, and treats a missing entry as a stop.
final class WebViewWorkerHost: NativeWorkerEngine, Sendable {
    typealias Builder = @Sendable (ProductId, NativeWorkerAttempt) async throws -> TrUAPIWorkerRuntime

    private struct WebView {
        let startup: Task<Void, Never>
        var runtime: TrUAPIWorkerRuntime?
    }

    private let build: Builder
    private let views = OSAllocatedUnfairLock<[UInt64: WebView]>(initialState: [:])

    init(build: @escaping Builder) {
        self.build = build
    }

    func start(productId: ProductId, attempt: NativeWorkerAttempt) {
        let id = attempt.id()
        views.withLock { views in
            let startup = Task { [weak self, build] in
                do {
                    let view = try await build(productId, attempt)
                    let accepted = self?.views.withLock { views in
                        guard views[id] != nil else { return false }
                        views[id]?.runtime = view
                        return true
                    } ?? false
                    guard accepted else {
                        await view.dispose()
                        return
                    }
                    try await view.start { attempt.failed(reason: "WebView process terminated") }
                    _ = attempt.moduleStarted()
                } catch {
                    guard !Task.isCancelled else { return }
                    attempt.failed(reason: String(describing: error))
                }
            }
            views[id] = WebView(startup: startup)
        }
    }

    func stop(productId _: ProductId, attemptId: UInt64) {
        guard let view = views.withLock({ $0.removeValue(forKey: attemptId) }) else { return }
        view.startup.cancel()
        if let runtime = view.runtime { Task { await runtime.dispose() } }
    }
}
