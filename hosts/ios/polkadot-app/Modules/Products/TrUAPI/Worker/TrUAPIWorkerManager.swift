import Foundation
import os
import AsyncExtensions
import Products
import StructuredConcurrency
import TrUAPIHost
import UIKitExt

protocol TrUAPIWorkerManaging: AnyObject, Sendable {
    func claim(_ productId: ProductId) throws -> any TrUAPIWorkerClaiming
    func waitUntilReady(_ productId: ProductId) async throws
    func context(of productId: ProductId) -> ProductWorkerContext
    func executions(of productId: ProductId) -> AnyAsyncSequence<TrUAPIWorkerOperations?>
    func currentExecution(of productId: ProductId) -> TrUAPIWorkerOperations?
    func reload(productId: ProductId) async
}

/// Owns the platform views opened for the core's startup attempts.
final class TrUAPIWorkerManager: NativeWebViewWorkerHost, TrUAPIWorkerManaging, Sendable {
    typealias Builder = @Sendable (
        ProductId, ProductWorkerContext, NativeWorkerAttempt
    ) async throws -> TrUAPIWorkerRuntime

    private struct WebView {
        let startup: Task<Void, Never>
        var runtime: TrUAPIWorkerRuntime?
    }

    private let build: Builder
    private let runtime: @Sendable () throws -> TrUAPIHostRuntime
    private let presentationFallback: @MainActor @Sendable () -> ControllerBackedProtocol?
    private let views = OSAllocatedUnfairLock<[UInt64: WebView]>(initialState: [:])
    private let openContexts = OSAllocatedUnfairLock<[ProductId: ProductWorkerContext]>(initialState: [:])

    init(
        build: @escaping Builder,
        runtime: @escaping @Sendable () throws -> TrUAPIHostRuntime,
        presentationFallback: @escaping @MainActor @Sendable () -> ControllerBackedProtocol?
    ) {
        self.build = build
        self.runtime = runtime
        self.presentationFallback = presentationFallback
    }

    func start(productId: ProductId, attempt: NativeWorkerAttempt) {
        let id = attempt.id()
        views.withLock { views in
            let startup = Task {
                do {
                    let view = try await build(productId, context(of: productId), attempt)
                    let accepted = self.views.withLock { views in
                        guard views[id] != nil else { return false }
                        views[id]?.runtime = view
                        return true
                    }
                    guard accepted else {
                        await view.dispose()
                        return
                    }
                    try await view.start { attempt.failed(reason: "WebView process terminated") }
                } catch {
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

    func claim(_ productId: ProductId) throws -> any TrUAPIWorkerClaiming {
        try runtime().claimWorker(productId: productId)
    }

    func waitUntilReady(_ productId: ProductId) async throws {
        let states = try runtime().workerExecutionState(productId: productId)
        try await withTimeout(.seconds(30)) {
            for await state in states {
                switch state {
                case .running: return
                case let .failed(reason): throw HostRejection.Rejected(reason: reason)
                case .idle, .pending: continue
                }
            }
            throw CancellationError()
        }
    }

    func context(of productId: ProductId) -> ProductWorkerContext {
        openContexts.withLock { open in
            if let context = open[productId] { return context }

            let context = ProductWorkerContext(presentationFallback: presentationFallback)
            open[productId] = context
            return context
        }
    }

    func executions(of productId: ProductId) -> AnyAsyncSequence<TrUAPIWorkerOperations?> {
        let states = (try? runtime().workerExecutionState(productId: productId)) ?? AsyncStream { $0.finish() }
        return states
            .map { state -> TrUAPIWorkerOperations? in
                if case let .running(execution) = state { return execution }
                return nil
            }
            .eraseToAnyAsyncSequence()
    }

    func currentExecution(of productId: ProductId) -> TrUAPIWorkerOperations? {
        try? runtime().currentWorkerExecution(productId: productId)
    }

    func reload(productId: ProductId) async {
        try? runtime().reloadWorker(productId: productId)
    }
}
