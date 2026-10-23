import Foundation
import os
import StructuredConcurrency
import TrUAPIHost

// MARK: - Bodies

extension ChatRemoteMessageContent.FileMeta {
    /// The media item of the message that the product gets.
    func mediaItem(_ hopFile: ChatHopResource) -> ChatMediaItem {
        switch self {
        case let .image(meta):
            .image(resource: hopFile.wireResource, meta: meta.general.mediaMeta(
                width: meta.width,
                height: meta.height,
                thumbnail: meta.thumbnail,
                durationMs: nil
            ))
        case let .video(meta):
            // The picker gives the length of the video in seconds, as the native chat sends it.
            .video(resource: hopFile.wireResource, meta: meta.general.mediaMeta(
                width: nil,
                height: nil,
                thumbnail: meta.thumbnail,
                durationMs: UInt64(meta.duration) * 1_000
            ))
        case let .general(meta):
            .file(
                resource: hopFile.wireResource,
                meta: meta.mediaMeta(width: nil, height: nil, thumbnail: nil, durationMs: nil),
                fileName: ""
            )
        }
    }
}

extension ChatRemoteMessageContent.GeneralFileMeta {
    func mediaMeta(width: UInt32?, height: UInt32?, thumbnail: Data?, durationMs: UInt64?) -> ChatMediaMeta {
        ChatMediaMeta(
            mimeType: mimeType,
            sizeBytes: UInt64(fileSize),
            width: width,
            height: height,
            placeholder: thumbnail.flatMap { String(data: $0, encoding: .utf8) },
            alt: nil,
            durationMs: durationMs
        )
    }
}

extension ChatMessageBody {
    /// The file of a sent photo or video, as the room draws it when the product has the message.
    var sentFile: ChatRemoteMessageContent.FileVariant? {
        guard case let .richText(_, media) = self else { return nil }
        return media.nativeAttachments.first
    }
}

// MARK: - App

/// Uploads through the shared runtime of the app (`chat_hop_attachment`). The core holds the keys of the HOP nodes.
struct TrUAPIProductHopUploads: ProductHopUploading {
    private let runtime: @Sendable () throws -> TrUAPIHostRuntime
    private let references: @Sendable () throws -> any TrUAPIWorkerReferencing
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    /// The longest wait for the worker before an upload. The core reads the declarations of the room through the
    /// open execution of the worker (D36).
    private let executionWait: Duration
    private let logger: LoggerProtocol

    init(
        runtime: @escaping @Sendable () throws -> TrUAPIHostRuntime = Self.sharedRuntime,
        references: (@Sendable () throws -> any TrUAPIWorkerReferencing)? = nil,
        workers: @escaping @Sendable () -> (any TrUAPIWorkerManaging)? = Self.sharedWorkers,
        executionWait: Duration = .seconds(30),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.runtime = runtime
        self.references = references ?? { try runtime() }
        self.workers = workers
        self.executionWait = executionWait
        self.logger = logger
    }

    func upload(
        productId: String,
        roomId: String,
        bytes: Data,
        progress: @escaping @Sendable (_ sent: UInt64, _ total: UInt64) -> Void
    ) async throws -> ChatHopResource {
        let resource = try await runtime().hopChatAttachment(
            productId: productId,
            roomId: roomId,
            bytes: bytes,
            progress: progress
        )
        guard case let .hop(identifier, ticket, node, size) = resource else {
            throw ProductOutboxRefusal(reason: nil)
        }
        return ChatHopResource(identifier: identifier, ticket: ticket, node: node, size: size)
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
    static func sharedRuntime() throws -> TrUAPIHostRuntime {
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

extension ProductOutboxDelivery: ProductPaymentDelivering {
    func deliver(_ payment: ProductChatPayment) async throws -> ChatMessage? {
        let bot = registry.withLockUnchecked { $0 }?.getChatExtensionBot(for: payment.productId)
        guard let querying = bot as? ProductChatQuerying else {
            throw ProductUnavailable()
        }

        return try await ProductQueryingDelivery(querying: querying).deliver(payment)
    }
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

/// Gives a payment to the product: `SendPayment` under the id of the payment, which the coinage records of the host
/// keep too. The product answers a second send of the same id with the same message.
extension ProductQueryingDelivery: ProductPaymentDelivering {
    func deliver(_ payment: ProductChatPayment) async throws -> ChatMessage? {
        do {
            let answer = try await querying.userAction(ProductChatUserActionRequest(
                roomId: payment.roomId,
                action: .sendPayment(
                    messageId: payment.messageId,
                    timestamp: payment.timestamp,
                    totalValue: String(payment.totalValue),
                    coinKeys: payment.coinKeys
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

/// No HOP node and no worker: an outbox in memory sends texts only.
struct NoProductHopUploads: ProductHopUploading {
    func upload(
        productId _: String,
        roomId _: String,
        bytes _: Data,
        progress _: @escaping @Sendable (_ sent: UInt64, _ total: UInt64) -> Void
    ) async throws -> ChatHopResource {
        throw ProductOutboxRefusal(reason: nil)
    }

    func holdWorker(productId _: String) async {}

    func releaseWorker(productId _: String) {}

    func workerStarts(productId _: String) -> AsyncStream<Void> {
        AsyncStream { $0.finish() }
    }
}
