import Foundation
import os
import Products

/// A running product worker the manager owns. Disposed when the last consumer
/// releases its token.
protocol ProductWorkerRunning: Sendable {
    func dispose() async
}

/// Builds and starts a headless worker runtime for a product. Injected so the
/// manager is testable without the real WKWebView/JS engine.
protocol ProductWorkerFactory: Sendable {
    func startWorker(productId: ProductId) async throws -> ProductWorkerRunning
}

/// A held claim on a product's worker plus the outcome of starting it. The
/// worker survives while the token is held; the result carries the running
/// worker for a consumer that drives it (chat) or the error for a caller that
/// wants to surface it.
struct ProductWorkerLease {
    let token: ProductWorkerToken
    let result: Result<ProductWorkerRunning, Error>

    var worker: ProductWorkerRunning? {
        try? result.get()
    }
}

/// Ref-counts a product's worker. The worker starts on the first `acquire` and
/// is disposed once the last token is released. Consumers (chats screen, the
/// product's full-page screen, open host-api operations) each hold one token.
protocol ProductWorkerManaging: Sendable {
    /// Acquire the product's worker, starting it if this is the first consumer.
    /// Waits for the boot so the returned lease carries the start result. Balance
    /// with `ProductWorkerToken.unlock()` (or let the token deinit).
    func acquire(productId: ProductId) async -> ProductWorkerLease
}

/// A single claim on a product's worker. Releases exactly once, whichever comes
/// first: an explicit `unlock()` or the token being dropped.
final class ProductWorkerToken: @unchecked Sendable {
    private let released = OSAllocatedUnfairLock(initialState: false)
    private let onRelease: @Sendable () -> Void

    init(onRelease: @escaping @Sendable () -> Void) {
        self.onRelease = onRelease
    }

    func unlock() {
        let shouldRelease = released.withLock { released -> Bool in
            guard !released else { return false }
            released = true
            return true
        }

        guard shouldRelease else { return }

        onRelease()
    }

    deinit {
        unlock()
    }
}

enum ProductWorkerManagerError: Error {
    case startProducedNoResult
}

final class ProductWorkerManager: ProductWorkerManaging, @unchecked Sendable {
    private struct Entry {
        var refCount: Int = 0
        var result: Result<ProductWorkerRunning, Error>?
        /// Serializes start/dispose for this product so a dispose always runs
        /// after the start it undoes, and a re-acquire waits for a pending dispose.
        var lifecycle: Task<Void, Never>?
    }

    private let state = OSAllocatedUnfairLock(initialState: [ProductId: Entry]())
    private let factory: ProductWorkerFactory
    private let logger: LoggerProtocol

    init(factory: ProductWorkerFactory, logger: LoggerProtocol = Logger.shared) {
        self.factory = factory
        self.logger = logger
    }

    func acquire(productId: ProductId) async -> ProductWorkerLease {
        logger.debug("Acquiring worker for product \(productId)...")

        let token = retain(productId: productId)

        // Wait for the boot triggered by (or preceding) this acquire. The token
        // keeps the ref count above zero, so no dispose can run while we wait.
        let lifecycle = state.withLock { $0[productId]?.lifecycle }
        await lifecycle?.value

        let result = state.withLock { $0[productId]?.result }

        logger.debug("Acquired worker for product \(productId)")

        return ProductWorkerLease(
            token: token,
            result: result ?? .failure(ProductWorkerManagerError.startProducedNoResult)
        )
    }

    private func retain(productId: ProductId) -> ProductWorkerToken {
        state.withLock { entries in
            var entry = entries[productId] ?? Entry()
            entry.refCount += 1
            if entry.refCount == 1 {
                entry.lifecycle = chainStart(productId: productId, after: entry.lifecycle)
            }
            entries[productId] = entry
        }

        return ProductWorkerToken { [weak self] in
            self?.release(productId: productId)
        }
    }

    private func release(productId: ProductId) {
        state.withLock { entries in
            guard var entry = entries[productId] else { return }
            entry.refCount -= 1
            guard entry.refCount <= 0 else {
                entries[productId] = entry
                return
            }
            entry.refCount = 0
            entry.lifecycle = chainDispose(productId: productId, after: entry.lifecycle)
            entries[productId] = entry
        }
    }

    /// Appends a start step to the product's lifecycle chain. Runs only if the
    /// worker is still wanted once any prior step finished, and records the start
    /// result so `acquire` can propagate it.
    private func chainStart(productId: ProductId, after previous: Task<Void, Never>?) -> Task<Void, Never> {
        Task { [weak self] in
            await previous?.value
            guard let self else { return }

            let wanted = state.withLock { ($0[productId]?.refCount ?? 0) > 0 }
            guard wanted else { return }

            let result: Result<ProductWorkerRunning, Error>
            do {
                result = try await .success(factory.startWorker(productId: productId))
                logger.debug("Worker started for \(productId)")
            } catch {
                logger.error("Worker start failed for \(productId): \(error)")
                result = .failure(error)
            }

            let keptWorker = state.withLock { entries -> ProductWorkerRunning? in
                guard var entry = entries[productId], entry.refCount > 0 else {
                    return (try? result.get())
                }
                entry.result = result
                entries[productId] = entry
                return nil
            }

            // Started but no longer wanted: dispose the worker we just built.
            await keptWorker?.dispose()
        }
    }

    /// Appends a dispose step. Runs after any in-flight start and tears the
    /// worker down. The entry is kept (`result == nil`, `refCount == 0`) so its
    /// `lifecycle` stays the single serialized chain for this product: a
    /// re-acquire appends to it rather than starting a second, parallel chain
    /// that could drop a just-stored worker before its paired dispose runs.
    private func chainDispose(productId: ProductId, after previous: Task<Void, Never>?) -> Task<Void, Never> {
        Task { [weak self] in
            await previous?.value
            guard let self else { return }

            let worker = state.withLock { entries -> ProductWorkerRunning? in
                let worker = try? entries[productId]?.result?.get()
                entries[productId]?.result = nil
                return worker
            }
            await worker?.dispose()

            logger.debug("Worker disposed for \(productId)")
        }
    }
}
