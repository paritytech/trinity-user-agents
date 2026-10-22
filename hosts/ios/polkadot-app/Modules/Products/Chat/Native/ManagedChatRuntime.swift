import Foundation
import os
import Products
import UIKitExt

enum ManagedChatRuntimeError: Error {
    case workerUnavailable(ProductId)
}

/// Native chat runtime backed by the shared ``ProductWorkerManager``. Instead of
/// booting its own JS engine, it locks the product's one managed worker and
/// drives it, binding chat messaging for as long as the chat surface is alive.
///
/// The lock is taken lazily on first use and released on `dispose`, so the chats
/// screen is just another consumer of the ref-counted worker: the worker boots
/// once for whoever needs it first (chat, SPA, or an operation) and is torn down
/// when the last of them lets go.
final class ManagedChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    private struct State {
        var leaseTask: Task<ProductWorkerLease, Never>?
        var disposed = false
    }

    private let productId: ProductId
    private let manager: ProductWorkerManaging
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(productId: ProductId, manager: ProductWorkerManaging) {
        self.productId = productId
        self.manager = manager
    }

    func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
        guard let worker = await ensureWorker() else {
            throw ManagedChatRuntimeError.workerUnavailable(productId)
        }

        worker.bindMessaging(messagingSupport)
        try await worker.onBotStarted()
    }

    func onUserMessage(text: String, roomId: String?) async throws {
        guard let worker = await ensureWorker() else {
            throw ManagedChatRuntimeError.workerUnavailable(productId)
        }

        try await worker.onUserMessage(text: text, roomId: roomId)
    }

    func renderMessage(
        roomId _: String?,
        messageId: String,
        messageType: String,
        messageData: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        guard let worker = await ensureWorker() else {
            return AsyncThrowingStream { $0.finish(throwing: ManagedChatRuntimeError.workerUnavailable(productId)) }
        }

        let hexStream = await worker.renderMessage(
            messageId: messageId,
            messageType: messageType,
            messageData: messageData
        )

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    for try await hexString in hexStream {
                        continuation.yield(.scaleEncoded(hexString))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    func dispatchEvent(
        roomId: String?,
        messageId: String,
        messageType _: String?,
        actionId: String,
        payload: String?
    ) async {
        guard let worker = await ensureWorker() else { return }

        await worker.dispatchEvent(
            roomId: roomId,
            messageId: messageId,
            actionId: actionId,
            payload: payload
        )
    }

    @MainActor
    func attach(presentationView view: ControllerBackedProtocol) {
        Task { [weak self] in
            guard let worker = await self?.ensureWorker() else { return }
            await MainActor.run { worker.attach(presentationView: view) }
        }
    }

    func dispose() async {
        let task = state.withLock { state -> Task<ProductWorkerLease, Never>? in
            state.disposed = true
            let task = state.leaseTask
            state.leaseTask = nil
            return task
        }

        guard let task else { return }

        let lease = await task.value
        (lease.worker as? ProductChatWorking)?.unbindMessaging()
        lease.token.unlock()
    }
}

private extension ManagedChatRuntime {
    /// Single-flights the lock: concurrent callers await the same acquire, and a
    /// disposed runtime never takes a new lock it would leak.
    func ensureWorker() async -> ProductChatWorking? {
        let task = state.withLock { state -> Task<ProductWorkerLease, Never>? in
            guard !state.disposed else { return nil }

            if let task = state.leaseTask {
                return task
            }

            let task = Task { [manager, productId] in await manager.acquire(productId: productId) }
            state.leaseTask = task
            return task
        }

        guard let task else { return nil }

        return await task.value.worker as? ProductChatWorking
    }
}
