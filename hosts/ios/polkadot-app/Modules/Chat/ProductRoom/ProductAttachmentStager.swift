import Foundation
import os
import TrUAPIHost

/// Why the host did not send a staged attachment (P10.3).
enum ProductStageFailure: Error, Equatable, Sendable {
    /// The core did not see that the room declares `preimage_attachments`. The core reads the declaration through
    /// the open worker of the product, so a retry can succeed when the worker runs.
    case notDeclared
    /// The attachment is larger than the core stages, or than one Bulletin transaction stores (O32).
    case tooLarge(maxBytes: Int)
    /// The user refused `PreimageSubmit` for the product.
    case permissionDenied
    case allowanceUnavailable(reason: String)
    case submitFailed(reason: String)
    /// The product refused the message. The reason is for the user.
    case refused(reason: String?)
    /// The message did not reach the product: no answer in time, or the product does not run.
    case notDelivered(reason: String)

    /// A refusal that a retry does not change. The attachment goes away, and the user sees why.
    var isFinal: Bool {
        switch self {
        case .tooLarge,
             .permissionDenied,
             .refused:
            true
        case .notDeclared,
             .allowanceUnavailable,
             .submitFailed,
             .notDelivered:
            false
        }
    }

    /// The text of the log line and of the outbox entry.
    var summary: String {
        switch self {
        case .notDeclared: "not declared"
        case let .tooLarge(maxBytes): "too large (max \(maxBytes))"
        case .permissionDenied: "permission denied"
        case let .allowanceUnavailable(reason): "allowance unavailable: \(reason)"
        case let .submitFailed(reason): "submit failed: \(reason)"
        case let .refused(reason): "refused: \(reason ?? "")"
        case let .notDelivered(reason): "not delivered: \(reason)"
        }
    }
}

/// The core call that stages the bytes, and the worker hold around it. The app never holds the allowance key: the
/// core stages through the Bulletin allowance of the product.
protocol ProductAttachmentStaging: Sendable {
    /// Stages the bytes and gives the preimage key. Throws ``ProductStageFailure``.
    func stage(productId: String, roomId: String, mimeType: String, bytes: Data) async throws -> Data
    /// Seals the bytes and gives the sealed file. Throws ``ProductStageFailure``.
    func seal(productId: String, roomId: String, bytes: Data) async throws -> ChatSealedResource
    /// Keeps the worker of the product while the host stages and sends, also after the room screen closes. Returns
    /// when the worker runs, or after a bound. The core reads the declaration of the room through the open execution
    /// (D36), so a stage before the worker runs can fail with `NotDeclared`.
    func holdWorker(productId: String) async
    func releaseWorker(productId: String)
}

/// What the stager does in the room of an entry. The two stores of a room do it in different ways.
protocol ProductOutboxDelivering: Sendable {
    /// Shows the attachment as a message on its way, or as a failed message. `preview` is the body of the bubble.
    func show(_ entry: ProductOutboxEntry, preview: ChatMessageBody) async
    /// Sends the message with the staged media. Gives the message that the product made in a room that it keeps.
    /// Throws ``ProductStageFailure``.
    func deliver(_ entry: ProductOutboxEntry, body: ChatMessageBody) async throws -> ChatMessage?
    /// Removes the message of the attachment, after a delete or a final refusal.
    func remove(_ entry: ProductOutboxEntry) async
}

/// What happened to an outbox entry, for the room screen and the window of a product-source room.
enum ProductOutboxEvent: Equatable {
    /// A new entry, or a new state. `preview` is the body of its bubble.
    case changed(ProductOutboxEntry, preview: ChatMessageBody)
    /// The product has the message. `created` is the message that the product made in a room that it keeps, and
    /// `body` is the body that the product received.
    case delivered(ProductOutboxEntry, created: ChatMessage?, body: ChatMessageBody)
    /// The entry went away: the user deleted it, or a final refusal came. Then `failure` tells why.
    case removed(ProductOutboxEntry, failure: ProductStageFailure?)
}

/// Stages the attachments of the user above the inline limit, and sends their messages (P10.3, 07-LATER §7).
///
/// The stager keeps each attachment in the outbox until the product has the message. The work does not depend on
/// the room screen: it continues after the screen closes, and an entry that failed stays in the outbox until the
/// user retries or deletes it. An entry that a closed app interrupted shows as failed after the next start.
actor ProductAttachmentStager {
    private let outbox: ProductOutboxStoring
    private let staging: ProductAttachmentStaging
    private let delivery: ProductOutboxDelivering
    private let media: ProductStagedMediaRegistry
    private let primeResolver: @Sendable (ChatResource, Data) async -> Void
    private let now: @Sendable () -> UInt64
    private let logger: LoggerProtocol

    private var tasks: [String: Task<Void, Never>] = [:]
    private var listeners: [UUID: AsyncStream<ProductOutboxEvent>.Continuation] = [:]
    private var interruptedChecked = false
    private var interruptedShown = false
    /// The bound of the Bulletin chain that the core gave in a `TooLarge` refusal, by product.
    private var liveBounds: [String: Int] = [:]

    init(
        outbox: ProductOutboxStoring,
        staging: ProductAttachmentStaging,
        delivery: ProductOutboxDelivering,
        media: ProductStagedMediaRegistry = .shared,
        primeResolver: @escaping @Sendable (ChatResource, Data) async -> Void = { _, _ in },
        now: @escaping @Sendable () -> UInt64 = { Date().toChatTimestamp() },
        logger: LoggerProtocol = Logger.shared
    ) {
        self.outbox = outbox
        self.staging = staging
        self.delivery = delivery
        self.media = media
        self.primeResolver = primeResolver
        self.now = now
        self.logger = logger
    }

    // MARK: - Events

    /// The events of every entry, from now on.
    func events() -> AsyncStream<ProductOutboxEvent> {
        let id = UUID()
        let (stream, continuation) = AsyncStream<ProductOutboxEvent>.makeStream(bufferingPolicy: .unbounded)
        listeners[id] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.removeListener(id) }
        }
        return stream
    }

    // MARK: - Queries

    /// The entries of a room, oldest first. An entry that a closed app interrupted is failed.
    func entries(productId: String, roomId: String) -> [ProductOutboxEntry] {
        markInterruptedEntries()
        return outbox.entries().filter { $0.productId == productId && $0.roomId == roomId }
    }

    /// The entries of a room with the bodies of their bubbles, for the window of a product-source room.
    func previews(productId: String, roomId: String) async -> [(entry: ProductOutboxEntry, preview: ChatMessageBody)] {
        var previews: [(entry: ProductOutboxEntry, preview: ChatMessageBody)] = []
        for entry in entries(productId: productId, roomId: roomId) {
            let bytes = outbox.bytes(of: entry.id) ?? Data()
            await previews.append((
                entry,
                ProductOutboxPreview.body(of: entry, bytes: bytes, fileURL: outbox.fileURL(of: entry.id))
            ))
        }
        return previews
    }

    /// True when the message is an attachment of the outbox.
    func owns(_ messageId: String) -> Bool {
        outbox.entry(messageId) != nil
    }

    /// The largest attachment that the product can stage now: the bound of the Bulletin chain when the core gave
    /// one, else the bound of the core.
    func stagingLimit(productId: String) -> Int {
        min(
            liveBounds[productId] ?? ProductAttachmentStagingPolicy.coreMaxBytes,
            ProductAttachmentStagingPolicy.coreMaxBytes
        )
    }

    // MARK: - Actions

    /// Puts the attachment in the outbox and starts to stage it, or to seal it when `seals` is true. Gives the entry,
    /// whose id is the id of the message.
    @discardableResult
    func send(
        _ attachment: ProductAttachment,
        productId: String,
        roomId: String,
        store: ProductOutboxEntry.Store,
        seals: Bool = false
    ) throws -> ProductOutboxEntry {
        let facts = ProductStagedMediaFacts(mimeType: attachment.mimeType, data: attachment.data)
        let prefix = store == .product ? ProductMessageSource.localIdPrefix : ""
        let entry = ProductOutboxEntry(
            id: prefix + "staged-" + UUID().uuidString,
            productId: productId,
            roomId: roomId,
            store: store,
            kind: ProductOutboxEntry.Kind(attachment.kind),
            mimeType: attachment.mimeType,
            sizeBytes: attachment.data.count,
            width: facts.width,
            height: facts.height,
            placeholder: facts.blurHash,
            createdAt: now(),
            key: nil,
            state: .sending,
            seals: seals
        )
        try outbox.save(entry, bytes: attachment.data)
        start(entry.id)
        return entry
    }

    /// Stages a failed entry again, or sends its message again when the core staged it already.
    @discardableResult
    func retry(_ id: String) -> Bool {
        guard tasks[id] == nil, let entry = outbox.entry(id), entry.isFailed else {
            return false
        }
        start(id)
        return true
    }

    /// Removes the entry, its bytes and its message. A preimage that the core staged stays on the Bulletin chain.
    @discardableResult
    func delete(_ id: String) async -> Bool {
        guard let entry = outbox.entry(id) else {
            return false
        }
        tasks[id]?.cancel()
        tasks[id] = nil
        outbox.remove(id)
        await delivery.remove(entry)
        emit(.removed(entry, failure: nil))
        return true
    }

    /// Shows each entry that a closed app interrupted as failed, also in the database of a host-store room, so that
    /// the user can retry or delete it. A room screen calls it when it opens.
    func resumeInterrupted() async {
        guard !interruptedShown else { return }
        interruptedShown = true
        markInterruptedEntries()

        for entry in outbox.entries() where entry.state == .failed(reason: "interrupted") && tasks[entry.id] == nil {
            let bytes = outbox.bytes(of: entry.id) ?? Data()
            let preview = await ProductOutboxPreview.body(
                of: entry,
                bytes: bytes,
                fileURL: outbox.fileURL(of: entry.id)
            )
            await delivery.show(entry, preview: preview)
            emit(.changed(entry, preview: preview))
        }
    }

    /// Waits until the work on the entry ends. For tests and for the E2E harness.
    func waitUntilIdle(_ id: String) async {
        await tasks[id]?.value
    }
}

// MARK: - Pipeline

private extension ProductAttachmentStager {
    func start(_ id: String) {
        tasks[id] = Task { [weak self] in
            await self?.run(id)
            await self?.finish(id)
        }
    }

    func finish(_ id: String) {
        tasks[id] = nil
    }

    func run(_ id: String) async {
        guard var entry = outbox.entry(id), let bytes = outbox.bytes(of: id) else {
            return
        }

        entry.state = .sending
        try? outbox.update(entry)
        let preview = await ProductOutboxPreview.body(of: entry, bytes: bytes, fileURL: outbox.fileURL(of: id))
        await delivery.show(entry, preview: preview)
        emit(.changed(entry, preview: preview))

        await staging.holdWorker(productId: entry.productId)
        defer { staging.releaseWorker(productId: entry.productId) }

        do {
            let resource = try await stagedResource(of: &entry, bytes: bytes)
            // The user can delete the entry while the core stages it.
            guard outbox.entry(id) != nil, !Task.isCancelled else {
                return
            }
            let body = ProductOutboxPreview.sentBody(of: entry, resource: resource)
            let created = try await delivery.deliver(entry, body: body)
            outbox.remove(id)
            logger.debug("[staging] \(entry.roomId) delivered \(entry.id)")
            emit(.delivered(entry, created: created, body: body))
        } catch {
            await fail(entry, error: error)
        }
    }

    func stagedResource(of entry: inout ProductOutboxEntry, bytes: Data) async throws -> TrUAPIChatResource {
        guard entry.seals == true else {
            return try await .preimage(key: stagedKey(of: &entry, bytes: bytes))
        }
        if let sealed = entry.sealed {
            return sealed.wireResource
        }

        let started = ContinuousClock.now
        let sealed = try await staging.seal(productId: entry.productId, roomId: entry.roomId, bytes: bytes)
        let elapsed = started.duration(to: .now)
        logger
            .debug(
                "[staging] \(entry.roomId) sealed \(entry.sizeBytes) B in \(sealed.parts.count) parts in \(elapsed.milliseconds) ms"
            )

        entry.sealed = sealed
        try? outbox.update(entry)
        media.register(key: sealed.digest, meta: ProductStagedMedia(entry))
        await primeResolver(.sealed(sealed), bytes)
        return sealed.wireResource
    }

    func stagedKey(of entry: inout ProductOutboxEntry, bytes: Data) async throws -> Data {
        if let key = entry.key {
            return key
        }

        let started = ContinuousClock.now
        let key = try await staging.stage(
            productId: entry.productId,
            roomId: entry.roomId,
            mimeType: entry.mimeType,
            bytes: bytes
        )
        let elapsed = started.duration(to: .now)
        logger.debug("[staging] \(entry.roomId) staged \(entry.sizeBytes) B in \(elapsed.milliseconds) ms")

        entry.key = key
        try? outbox.update(entry)
        media.register(key: key, meta: ProductStagedMedia(entry))
        await primeResolver(.preimage(key: key), bytes)
        return key
    }

    func fail(_ entry: ProductOutboxEntry, error: Error) async {
        let failure = (error as? ProductStageFailure) ?? .notDelivered(reason: String(describing: error))
        logger.warning("[staging] \(entry.roomId) \(entry.id) failed: \(failure.summary)")

        guard outbox.entry(entry.id) != nil else {
            return
        }

        if case let .tooLarge(maxBytes) = failure {
            liveBounds[entry.productId] = maxBytes
        }

        guard !failure.isFinal else {
            outbox.remove(entry.id)
            await delivery.remove(entry)
            emit(.removed(entry, failure: failure))
            return
        }

        var failed = entry
        failed.state = .failed(reason: failure.summary)
        try? outbox.update(failed)
        let bytes = outbox.bytes(of: entry.id) ?? Data()
        let preview = await ProductOutboxPreview.body(of: failed, bytes: bytes, fileURL: outbox.fileURL(of: entry.id))
        await delivery.show(failed, preview: preview)
        emit(.changed(failed, preview: preview))
    }

    /// An entry in the `sending` state with no task is from an app that closed. It shows as failed.
    func markInterruptedEntries() {
        guard !interruptedChecked else { return }
        interruptedChecked = true

        for var entry in outbox.entries() where !entry.isFailed && tasks[entry.id] == nil {
            entry.state = .failed(reason: "interrupted")
            try? outbox.update(entry)
        }
    }

    func emit(_ event: ProductOutboxEvent) {
        for listener in listeners.values {
            listener.yield(event)
        }
    }

    func removeListener(_ id: UUID) {
        listeners[id] = nil
    }
}

private extension ProductOutboxEntry.Kind {
    init(_ kind: ProductAttachment.Kind) {
        switch kind {
        case .image: self = .image
        case .video: self = .video
        case let .file(name): self = .file(name: name)
        }
    }
}

private extension Duration {
    var milliseconds: Int64 {
        components.seconds * 1_000 + components.attoseconds / 1_000_000_000_000_000
    }
}
