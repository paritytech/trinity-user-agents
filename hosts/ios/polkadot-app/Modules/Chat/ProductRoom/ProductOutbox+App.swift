import Foundation
import os
import StructuredConcurrency
import TrUAPIHost

// MARK: - App

/// Holds the worker of the product through the shared runtime of the app while the outbox sends.
struct TrUAPIProductOutboxWorkers: ProductOutboxWorkers {
    private let references: @Sendable () throws -> any TrUAPIWorkerReferencing
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    /// The longest wait for the worker before a send.
    private let executionWait: Duration
    private let logger: LoggerProtocol

    init(
        references: @escaping @Sendable () throws -> any TrUAPIWorkerReferencing = Self.sharedRuntime,
        workers: @escaping @Sendable () -> (any TrUAPIWorkerManaging)? = Self.sharedWorkers,
        executionWait: Duration = .seconds(30),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.references = references
        self.workers = workers
        self.executionWait = executionWait
        self.logger = logger
    }

    /// Takes a worker reference, and then waits until the execution of the worker runs.
    func holdWorker(productId: String) async {
        do {
            try references().acquireWorker(productId: productId)
        } catch {
            logger.warning("[outbox] cannot hold the worker of \(productId): \(error)")
            return
        }

        guard let workers = workers(), workers.currentExecution(of: productId) == nil else {
            return
        }

        do {
            try await withTimeout(executionWait) {
                for try await execution in workers.executions(of: productId) where execution != nil {
                    return
                }
            }
        } catch {
            logger.warning("[outbox] the worker of \(productId) does not run: \(error)")
        }
    }

    func releaseWorker(productId: String) {
        try? references().releaseWorker(productId: productId)
    }

    func workerStarts(productId: String) -> AsyncStream<Void> {
        guard let workers = workers() else {
            return AsyncStream { $0.finish() }
        }

        let executions = workers.executions(of: productId)
        return AsyncStream { continuation in
            let task = Task {
                var wasRunning = false
                do {
                    for try await execution in executions {
                        let isRunning = execution != nil
                        if isRunning, !wasRunning {
                            continuation.yield()
                        }
                        wasRunning = isRunning
                    }
                } catch {}
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    @Sendable
    static func sharedRuntime() throws -> any TrUAPIWorkerReferencing {
        guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            throw ProductOutboxDelivery.ProductUnavailable()
        }
        return try provider.sharedRuntime()
    }

    /// The worker manager that the runtime provider of the app has.
    @Sendable
    static func sharedWorkers() -> (any TrUAPIWorkerManaging)? {
        let provider: TrUAPIHostRuntimeProviding? = RootDependencyLocator.getDependency()
        return provider?.attachedWorkerManager
    }
}

/// Gives the messages of the outbox to the product of the room through its bot (`user_action`).
final class ProductOutboxDelivery: ProductOutboxDelivering, @unchecked Sendable {
    static let shared = ProductOutboxDelivery()

    fileprivate let registry = OSAllocatedUnfairLock<ChatExtensionsRegistering?>(uncheckedState: nil)

    /// The registry that gives the bot of each product. The app attaches it when it starts.
    func attach(registry: ChatExtensionsRegistering) {
        self.registry.withLockUnchecked { $0 = registry }
    }

    func deliver(_ message: ProductOutgoingMessage, body: ChatMessageBody) async throws -> ChatMessage? {
        let bot = registry.withLockUnchecked { $0 }?.getChatExtensionBot(for: message.productId)
        guard let querying = bot as? ProductChatQuerying else {
            throw ProductUnavailable()
        }

        return try await ProductQueryingDelivery(querying: querying).deliver(message, body: body)
    }

    /// The product does not run now.
    struct ProductUnavailable: Error {}
}

/// Gives the messages to the product through its queries: `NewMessage` under the id of the message.
struct ProductQueryingDelivery: ProductOutboxDelivering {
    let querying: ProductChatQuerying

    func deliver(_ message: ProductOutgoingMessage, body: ChatMessageBody) async throws -> ChatMessage? {
        do {
            let answer = try await querying.userAction(ProductChatUserActionRequest(
                roomId: message.roomId,
                action: .newMessage(
                    body: body,
                    replyTo: message.replyTo,
                    messageId: message.id,
                    timestamp: message.createdAt
                )
            ))
            if case let .messageCreated(created) = answer {
                return created
            }
            return nil
        } catch let ChatQueryError.actionRefused(.rejected(reason)) {
            throw ProductOutboxRefusal(reason: reason)
        }
    }
}

/// No worker to hold: an outbox in memory sends through the queries of its room.
struct NoProductOutboxWorkers: ProductOutboxWorkers {
    func holdWorker(productId _: String) async {}

    func releaseWorker(productId _: String) {}

    func workerStarts(productId _: String) -> AsyncStream<Void> {
        AsyncStream { $0.finish() }
    }
}
