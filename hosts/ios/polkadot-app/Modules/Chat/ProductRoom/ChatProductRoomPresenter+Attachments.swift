import Foundation

/// An attachment that the user picked, before the host makes it inline.
enum PickedAttachment: Equatable {
    case image(Data)
    case video(Data, mimeType: String)
    case file(Data, name: String)
}

/// The attachments of a product room (P6.7, scenario S36): inline up to 1 MiB, with their metadata.
///
/// A room that declares `preimage_attachments`, in a host that lists the feature, stages a larger attachment as a
/// preimage that the Bulletin allowance of the product pays for (P10.3, scenarios S57 to S60). The stager keeps it
/// in the outbox until the product has the message. Before the first staged attachment of a product, the user
/// accepts that the file goes to public storage.
extension ChatProductRoomPresenter {
    /// Offers the kinds of attachment that the input of the room allows.
    func showAttachmentPicker() {
        guard case let .input(input)? = policy?.footer else { return }

        let kinds = input.attachmentKinds
        switch kinds.count {
        case 0:
            return
        case 1:
            pickAttachment(kinds[0])
        default:
            wireframe.chooseAttachmentKind(kinds, from: view) { [weak self] kind in
                self?.pickAttachment(kind)
            }
        }
    }

    func pickAttachment(_ kind: ChatAttachmentKind) {
        wireframe.pickAttachment(kind, from: view) { [weak self] picked in
            self?.sendAttachment(picked)
        }
    }

    /// True when the room stages an attachment above the inline limit: the room declares the feature, and this
    /// host lists it.
    var stagesAttachments: Bool {
        policy?.features.preimageAttachments == true && staging.hostListsFeature
    }

    /// Makes the attachment inline, or stages it, and sends it as a message of the user. In a room that the host
    /// stores, the product receives `MessagePosted`. In a room that the product keeps, it receives `NewMessage`.
    ///
    /// In a room that does not stage attachments, a video above the inline limit is exported at 640x480 first
    /// (``ProductVideoCompressor``). A video that is still too large is refused with its size.
    func sendAttachment(_ picked: PickedAttachment) {
        if case let .video(data, _) = picked, !stagesAttachments, data.count > ProductAttachmentEncoder.inlineLimit {
            compressAndSend(data)
            return
        }

        let route: ProductAttachmentRoute
        do {
            route = try ProductAttachmentEncoder.route(
                picked,
                staging: ProductAttachmentStagingPolicy(allowed: stagesAttachments, maxBytes: staging.limit)
            )
        } catch let ProductAttachmentError.tooLarge(bytes) {
            logger.debug("Attachment of \(bytes) bytes refused in room \(roomId)")
            mark("failure", "attachment too large \(bytes)")
            view?.showActionFailure(String(localized: .ChatExtension.attachmentTooLarge))
            return
        } catch let ProductAttachmentError.aboveStagingLimit(bytes, limit) {
            logger.debug("Attachment of \(bytes) bytes is above the staging limit \(limit) in room \(roomId)")
            mark("failure", "attachment above staging limit \(bytes) max=\(limit)")
            view?.showActionFailure(Self.aboveLimitMessage(limit))
            return
        } catch {
            view?.showActionFailure(String(localized: .ChatExtension.attachmentUnreadable))
            return
        }

        switch route {
        case let .inline(attachment):
            sendInline(attachment)
        case let .staged(attachment):
            confirmPublicStorage { [weak self] in
                self?.stage(attachment)
            }
        }
    }

    private func compressAndSend(_ data: Data) {
        mark("video", "compress \(data.count)")
        Task { [weak self] in
            do {
                let smaller = try await ProductVideoCompressor.compress(
                    data,
                    limit: ProductAttachmentEncoder.inlineLimit
                )
                self?.mark("video", "compressed \(data.count) to \(smaller.count)")
                self?.sendAttachment(.video(smaller, mimeType: "video/mp4"))
            } catch let ProductVideoCompressor.Failure.tooLarge(bytes, limit) {
                self?.mark("failure", "video too large \(bytes) max=\(limit)")
                self?.view?.showActionFailure(String(localized: .ChatExtension.attachmentVideoTooLarge(
                    limit: ByteCountFormatter.string(fromByteCount: Int64(limit), countStyle: .binary)
                )))
            } catch {
                self?.view?.showActionFailure(String(localized: .ChatExtension.attachmentUnreadable))
            }
        }
    }

    private func sendInline(_ attachment: ProductAttachment) {
        guard let sendContent else { return }
        Task { [weak self] in
            do {
                try await sendContent(attachment.content)
            } catch let refusal as ChatUserActionRefusal {
                self?.view?.showActionFailure(refusal.reason ?? String(localized: .ChatExtension.actionRefused))
            } catch {
                self?.logger.error("Attachment of room \(self?.roomId ?? "") not sent: \(error)")
            }
        }
    }

    /// Before the first staged attachment of the product, the user accepts that files over 1 MiB go to public
    /// storage without encryption (D27). A refusal keeps the attachment unsent.
    private func confirmPublicStorage(then send: @escaping () -> Void) {
        let productId = appearanceFactory.productId
        guard !staging.notice.wasAccepted(productId: productId) else {
            send()
            return
        }

        staging.noticeCount += 1
        mark("notice", "shown n=\(staging.noticeCount)")
        wireframe.confirmPublicStorage(productName: appearanceFactory.productName, from: view) { [weak self] accepted in
            guard let self else { return }
            mark("notice", "\(accepted ? "accepted" : "declined") n=\(staging.noticeCount)")
            guard accepted else { return }
            staging.notice.accept(productId: productId)
            send()
        }
    }

    private func stage(_ attachment: ProductAttachment) {
        let store: ProductOutboxEntry.Store = policy?.messageStore == .product ? .product : .host
        Task { [weak self, staging, roomId, appearanceFactory] in
            do {
                try await staging.stager.send(
                    attachment,
                    productId: appearanceFactory.productId,
                    roomId: roomId,
                    store: store
                )
            } catch {
                self?.logger.error("Attachment of room \(roomId) not put in the outbox: \(error)")
                self?.view?.showActionFailure(String(localized: .ChatExtension.attachmentUnreadable))
            }
        }
    }

    /// Shows what happens to the staged attachments of this room: the markers of the E2E harness, and the reason
    /// of a final refusal.
    func followStaging() {
        staging.follow { [weak self] event in
            self?.didReceive(event)
        }
    }

    private func didReceive(_ event: ProductOutboxEvent) {
        switch event {
        case let .changed(entry, _):
            guard entry.roomId == roomId, entry.productId == appearanceFactory.productId else { return }
            switch entry.state {
            case .sending:
                markStaging("sending \(entry.kindName) \(entry.sizeBytes)")
            case let .failed(reason):
                markStaging("failed \(entry.kindName) \(entry.sizeBytes) \(reason)")
            }
        case let .delivered(entry, _, _):
            guard entry.roomId == roomId, entry.productId == appearanceFactory.productId else { return }
            markStaging("delivered \(entry.kindName) \(entry.sizeBytes)")
        case let .removed(entry, failure):
            guard entry.roomId == roomId, entry.productId == appearanceFactory.productId else { return }
            if case let .tooLarge(maxBytes) = failure {
                staging.limit = min(staging.limit, maxBytes)
            }
            markStaging("removed \(entry.kindName) \(entry.sizeBytes) \(failure?.summary ?? "deleted")")
            if let message = failure.flatMap(failureMessage) {
                view?.showActionFailure(message)
            }
        }
    }

    private func markStaging(_ value: String) {
        staging.markCount += 1
        mark("staging", "n=\(staging.markCount) \(value)")
        #if targetEnvironment(simulator)
            Task { [weak self, staging, roomId, appearanceFactory] in
                let entries = await staging.stager.entries(productId: appearanceFactory.productId, roomId: roomId)
                self?.mark("outbox", "n=\(staging.markCount) entries=\(entries.count)")
            }
        #endif
    }

    private func failureMessage(_ failure: ProductStageFailure) -> String? {
        switch failure {
        case let .tooLarge(maxBytes):
            Self.aboveLimitMessage(maxBytes)
        case .permissionDenied:
            String(localized: .ChatExtension.attachmentStagingPermissionDenied(product: appearanceFactory.productName))
        case let .refused(reason):
            reason ?? String(localized: .ChatExtension.actionRefused)
        case .notDeclared,
             .allowanceUnavailable,
             .submitFailed,
             .notDelivered:
            nil
        }
    }

    static func aboveLimitMessage(_ limit: Int) -> String {
        String(localized: .ChatExtension.attachmentAboveStagingLimit(
            limit: ByteCountFormatter.string(fromByteCount: Int64(limit), countStyle: .binary)
        ))
    }
}

/// The staging state of one room screen: the stager, the notice, the limit that the core gave, and the follow of
/// the outbox events.
@MainActor
final class ProductRoomStaging {
    let stager: ProductAttachmentStager
    let notice: ProductStagingNotice
    /// The host lists `preimage_attachments` in its capabilities.
    let hostListsFeature: Bool
    /// The largest attachment that the room stages. It goes down to the Bulletin bound when the core gives one.
    var limit = ProductAttachmentStagingPolicy.coreMaxBytes
    /// Counts for the markers of the E2E harness, so that each marker has a new value.
    var noticeCount = 0
    var markCount = 0
    private var task: Task<Void, Never>?

    init(
        stager: ProductAttachmentStager = .shared,
        notice: ProductStagingNotice = ProductStagingNotice(),
        hostListsFeature: Bool = ProductRoomStaging.hostListsStaging
    ) {
        self.stager = stager
        self.notice = notice
        self.hostListsFeature = hostListsFeature
    }

    deinit {
        task?.cancel()
    }

    /// The host lists `preimage_attachments`. A simulator build stages also when the E2E harness asks for it with
    /// `TRUAPI_IOS_E2E_STAGING=1`, because the host does not list the feature yet.
    nonisolated static var hostListsStaging: Bool {
        if ChatHostCapabilities.served.contains(ChatHostCapabilities.preimageAttachments) {
            return true
        }
        #if targetEnvironment(simulator)
            return TrUAPIE2EMarkers.isEnabled
                && ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_STAGING"] == "1"
        #else
            return false
        #endif
    }

    func follow(_ handle: @escaping @MainActor (ProductOutboxEvent) -> Void) {
        task?.cancel()
        task = Task { [stager] in
            let events = await stager.events()
            await stager.resumeInterrupted()
            for await event in events {
                await MainActor.run { handle(event) }
            }
        }
    }
}

/// Keeps, for each product, that the user accepted the public-storage notice (07-LATER §7.2).
struct ProductStagingNotice {
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func wasAccepted(productId: String) -> Bool {
        defaults.bool(forKey: Self.key(productId))
    }

    func accept(productId: String) {
        defaults.set(true, forKey: Self.key(productId))
    }

    /// Shows the notice again before the next staged attachment of the product.
    func reset(productId: String) {
        defaults.removeObject(forKey: Self.key(productId))
    }

    private static func key(_ productId: String) -> String {
        "chat.product.staging.notice." + productId
    }
}

private extension ProductOutboxEntry {
    var kindName: String {
        switch kind {
        case .image: "image"
        case .video: "video"
        case .file: "file"
        }
    }
}
