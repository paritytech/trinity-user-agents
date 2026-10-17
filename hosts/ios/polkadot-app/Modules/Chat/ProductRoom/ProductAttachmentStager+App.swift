import Foundation
import Operation_iOS
import os
import StructuredConcurrency
import TrUAPIHost

extension ProductAttachmentStager {
    /// The stager of the app. The room screens give it the registry of the chat extensions.
    static let shared = ProductAttachmentStager(
        outbox: ProductOutboxDirectory.shared,
        staging: TrUAPIProductAttachmentStaging(),
        delivery: ProductOutboxDelivery.shared,
        primeResolver: { resource, bytes in
            await ChatResourceResolver.shared.store(bytes, for: resource)
        }
    )
}

/// Stages through the shared runtime of the app (`chat_stage_attachment`). The core holds the allowance key and
/// reaches the Bulletin chain. The app gives it only the product, the room and the bytes.
struct TrUAPIProductAttachmentStaging: ProductAttachmentStaging {
    private let runtime: @Sendable () throws -> TrUAPIHostRuntime
    private let references: @Sendable () throws -> any TrUAPIWorkerReferencing
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    /// The longest wait for the worker before a stage. Android waits for the same execution (PARITY row 49).
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

    func stage(productId: String, roomId: String, mimeType: String, bytes: Data) async throws -> Data {
        #if targetEnvironment(simulator)
            if let fault = TrUAPIE2EStagingFault.take() {
                throw fault
            }
        #endif

        let runtime: TrUAPIHostRuntime
        do {
            runtime = try self.runtime()
        } catch {
            throw ProductStageFailure.submitFailed(reason: "the runtime is not ready: \(error)")
        }

        do {
            return try await runtime.stageChatAttachment(
                productId: productId,
                roomId: roomId,
                mimeType: mimeType,
                bytes: bytes
            )
        } catch let rejection as NativeChatStageRejection {
            throw ProductStageFailure(rejection)
        }
    }

    func seal(productId: String, roomId: String, bytes: Data) async throws -> ChatSealedResource {
        let runtime: TrUAPIHostRuntime
        do {
            runtime = try self.runtime()
        } catch {
            throw ProductStageFailure.submitFailed(reason: "the runtime is not ready: \(error)")
        }

        let resource: TrUAPIChatResource
        do {
            resource = try await runtime.sealChatAttachment(productId: productId, roomId: roomId, bytes: bytes)
        } catch let rejection as NativeChatStageRejection {
            throw ProductStageFailure(rejection)
        }
        guard let sealed = ChatSealedResource(resource) else {
            throw ProductStageFailure.submitFailed(reason: "the core gave no sealed file")
        }
        return sealed
    }

    /// Takes a worker reference, and then waits until the execution of the worker runs. On a cold start, the core
    /// can read the declaration of the room only through the open execution (D36).
    func holdWorker(productId: String) async {
        do {
            try references().acquireWorker(productId: productId)
        } catch {
            logger.warning("[staging] cannot hold the worker of \(productId): \(error)")
            return
        }

        await waitForExecution(of: productId)
    }

    func releaseWorker(productId: String) {
        try? references().releaseWorker(productId: productId)
    }

    /// Returns when the worker of the product runs, or after ``executionWait``. A stage after the bound still runs,
    /// and the core answers it.
    func waitForExecution(of productId: String) async {
        guard let workers = workers() else {
            logger.warning("[staging] no worker manager: the worker of \(productId) is not awaited")
            return
        }

        guard workers.currentExecution(of: productId) == nil else {
            return
        }

        do {
            try await withTimeout(executionWait) {
                for try await execution in workers.executions(of: productId) where execution != nil {
                    return
                }
            }
        } catch {
            logger.warning("[staging] the worker of \(productId) does not run: \(error)")
        }
    }

    @Sendable
    static func sharedRuntime() throws -> TrUAPIHostRuntime {
        guard let provider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            throw ProductStageFailure.submitFailed(reason: "no runtime provider")
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

extension ProductStageFailure {
    init(_ rejection: NativeChatStageRejection) {
        switch rejection {
        case .NotDeclared: self = .notDeclared
        case let .TooLarge(maxBytes): self = .tooLarge(maxBytes: Int(maxBytes))
        case .PermissionDenied: self = .permissionDenied
        case let .AllowanceUnavailable(reason): self = .allowanceUnavailable(reason: reason)
        case let .SubmitFailed(reason): self = .submitFailed(reason: reason)
        }
    }
}

/// The bot of a product that posts a staged attachment of a host-store room to the product.
protocol ProductStagedAttachmentPosting: AnyObject {
    func postStagedAttachment(
        messageId: String,
        body: Chat.LocalMessage.Content.ProductBody,
        roomId: String
    ) async
}

/// Writes the message of a staged attachment where the room keeps its messages.
///
/// - Host store: the database keeps the message from the start, as a message on its way (`new`) or a failed one.
///   When the core staged the bytes, the message gets the preimage media and the `delivered` status, and the
///   product receives `MessagePosted`.
/// - Product store: the window of the room shows the entry (``ProductMessageSource``). The product receives
///   `NewMessage` through `user_action`, and its answer replaces the entry.
final class ProductOutboxDelivery: ProductOutboxDelivering, @unchecked Sendable {
    static let shared = ProductOutboxDelivery()

    private let registry = OSAllocatedUnfairLock<ChatExtensionsRegistering?>(uncheckedState: nil)
    private let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        messageRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )
        self.logger = logger
    }

    /// The registry that gives the bot of each product. A room screen attaches it when it opens.
    func attach(registry: ChatExtensionsRegistering) {
        self.registry.withLockUnchecked { $0 = registry }
    }

    func show(_ entry: ProductOutboxEntry, preview: ChatMessageBody) async {
        guard entry.store == .host else { return }
        await save(entry, content: preview.localContent, status: .outgoing(entry.isFailed ? .failed : .new))
    }

    func deliver(_ entry: ProductOutboxEntry, body: ChatMessageBody) async throws -> ChatMessage? {
        let bot = registry.withLockUnchecked { $0 }?.getChatExtensionBot(for: entry.productId)

        switch entry.store {
        case .host:
            let content = body.localContent
            await save(entry, content: content, status: .outgoing(.delivered))
            guard let bot = bot as? ProductStagedAttachmentPosting, case let .productBody(productBody) = content else {
                throw ProductStageFailure.notDelivered(reason: "the product does not run")
            }
            await bot.postStagedAttachment(messageId: entry.id, body: productBody, roomId: entry.roomId)
            return nil
        case .product:
            guard let querying = bot as? ProductChatQuerying else {
                throw ProductStageFailure.notDelivered(reason: "the product does not run")
            }
            do {
                let answer = try await querying.userAction(ProductChatUserActionRequest(
                    roomId: entry.roomId,
                    action: .newMessage(body: body, replyTo: nil)
                ))
                if case let .messageCreated(created) = answer {
                    return created
                }
                return nil
            } catch let ChatQueryError.actionRefused(.rejected(reason)) {
                throw ProductStageFailure.refused(reason: reason)
            } catch {
                throw ProductStageFailure.notDelivered(reason: String(describing: error))
            }
        }
    }

    func remove(_ entry: ProductOutboxEntry) async {
        guard entry.store == .host else { return }
        do {
            try await messageRepository.saveOperation({ [] }, { [entry.id] }).asyncExecute()
        } catch {
            logger.error("Staged message \(entry.id) not removed: \(error)")
        }
    }

    private func save(
        _ entry: ProductOutboxEntry,
        content: Chat.LocalMessage.Content,
        status: Chat.LocalMessage.Status
    ) async {
        let message = Chat.LocalMessage(
            messageId: entry.id,
            chatId: entry.chatId,
            origin: .user,
            creationSource: .localDevice,
            status: status,
            timestamp: entry.createdAt,
            content: content,
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
        do {
            try await messageRepository.saveOperation({ [message] }, { [] }).asyncExecute()
        } catch {
            logger.error("Staged message \(entry.id) not stored: \(error)")
        }
    }
}

#if targetEnvironment(simulator)
    /// A failure that the next stage gives, for the truapi E2E harness (S59). The harness sets it with the host
    /// action `staging-fault`. The live chain does not fail on request.
    enum TrUAPIE2EStagingFault {
        private static let fault = OSAllocatedUnfairLock<ProductStageFailure?>(initialState: nil)

        static func set(_ failure: ProductStageFailure) {
            fault.withLock { $0 = failure }
        }

        static func take() -> ProductStageFailure? {
            fault.withLock { current in
                defer { current = nil }
                return current
            }
        }
    }
#endif
