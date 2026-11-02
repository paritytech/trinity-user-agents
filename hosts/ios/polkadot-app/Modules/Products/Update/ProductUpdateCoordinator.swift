import Foundation
import FoundationExt
import Products

@MainActor
protocol SPAProductReloading: AnyObject {
    var servedContentHash: String? { get }

    func serveAppliedBuild()
}

protocol ProductWorkerReloading {
    func reloadWorker(of productId: ProductId) async
}

@MainActor
protocol ProductPresenceRegistering: AnyObject {
    func register(
        productId: ProductId,
        contentId: ProductExecutable.Identifier,
        displayName: String,
        reloader: SPAProductReloading
    ) -> UUID
    func unregister(_ presenceId: UUID)
    func requestRefresh(of productId: ProductId)
    func applyPendingUpdate(of contentId: ProductExecutable.Identifier) -> Bool
}

@MainActor
final class ProductUpdateCoordinator {
    /// When products are refreshed, and which ones besides those on screen.
    struct RefreshSources {
        let applicationStateStreams: ApplicationStateStreamFactory
        let hostPlacedProducts: HostPlacedProductsProviding
    }

    private struct Presence {
        let productId: ProductId
        let contentId: ProductExecutable.Identifier
        let displayName: String
        weak var reloader: SPAProductReloading?
    }

    private let contentLoader: ProductContentLoading
    private let workerReloader: ProductWorkerReloading
    private let noticePresenter: ProductUpdateNoticePresenting
    private let refreshSources: RefreshSources
    private let logger: LoggerProtocol

    private var presences: [UUID: Presence] = [:]
    private var pendingUpdates: [ProductExecutable.Identifier: ExecutableRelease] = [:]
    private var dismissedContentIds: Set<ProductExecutable.Identifier> = []
    private var noticeContentId: ProductExecutable.Identifier?
    private var updatesTask: Task<Void, Never>?
    private var foregroundTask: Task<Void, Never>?

    init(
        contentLoader: ProductContentLoading,
        workerReloader: ProductWorkerReloading,
        noticePresenter: ProductUpdateNoticePresenting,
        refreshSources: RefreshSources,
        logger: LoggerProtocol
    ) {
        self.contentLoader = contentLoader
        self.workerReloader = workerReloader
        self.noticePresenter = noticePresenter
        self.refreshSources = refreshSources
        self.logger = logger
    }

    deinit {
        updatesTask?.cancel()
        foregroundTask?.cancel()
    }

    func start() {
        guard updatesTask == nil else { return }

        let updates = contentLoader.updates()
        updatesTask = Task { [weak self] in
            await self?.removeSupersededContent()

            do {
                for try await update in updates {
                    self?.handle(update)
                }
            } catch {
                self?.logger.error("Product updates stream ended: \(error)")
            }
        }

        let foregroundEvents = refreshSources.applicationStateStreams.stream(for: .willEnterForeground)
        foregroundTask = Task { [weak self] in
            for await _ in foregroundEvents {
                await self?.refreshProducts()
            }
        }
    }
}

extension ProductUpdateCoordinator: ProductPresenceRegistering {
    func register(
        productId: ProductId,
        contentId: ProductExecutable.Identifier,
        displayName: String,
        reloader: SPAProductReloading
    ) -> UUID {
        let presenceId = UUID()
        presences[presenceId] = Presence(
            productId: productId,
            contentId: contentId,
            displayName: displayName,
            reloader: reloader
        )
        return presenceId
    }

    func unregister(_ presenceId: UUID) {
        guard let presence = presences.removeValue(forKey: presenceId),
              !isPresented(presence.contentId),
              let update = pendingUpdates.removeValue(forKey: presence.contentId) else {
            return
        }

        dismissedContentIds.remove(presence.contentId)
        hideNotice(of: presence.contentId)
        apply(update, reloadingPresentations: false)
    }

    func requestRefresh(of productId: ProductId) {
        Task { [contentLoader] in
            await contentLoader.refresh(productId)
        }
    }

    func applyPendingUpdate(of contentId: ProductExecutable.Identifier) -> Bool {
        guard let update = pendingUpdates.removeValue(forKey: contentId) else {
            return false
        }

        dismissedContentIds.remove(contentId)
        hideNotice(of: contentId)
        apply(update, reloadingPresentations: true)

        return true
    }
}

private extension ProductUpdateCoordinator {
    func handle(_ update: ExecutableRelease) {
        guard isPresented(update.contentId) else {
            apply(update, reloadingPresentations: false)
            return
        }

        pendingUpdates[update.contentId] = update
        dismissedContentIds.remove(update.contentId)
        showNextNotice()
    }

    func apply(_ update: ExecutableRelease, reloadingPresentations: Bool) {
        let contentId = update.contentId

        logger.debug("Applying the update of \(contentId), reloading open SPAs: \(reloadingPresentations)")

        Task { [weak self, contentLoader, workerReloader] in
            await contentLoader.apply(update)

            if reloadingPresentations {
                self?.reloaders(of: contentId).forEach { $0.serveAppliedBuild() }
            }

            if let productId = Self.productIdOfWorker(servedFrom: contentId) {
                await workerReloader.reloadWorker(of: productId)
            }

            await self?.removeSupersededContent()
        }

        showNextNotice()
    }

    /// A legacy product runs its worker from its base name, so a base-name release reloads it too.
    static func productIdOfWorker(servedFrom contentId: ProductExecutable.Identifier) -> ProductId? {
        let productId = ProductManifestRecords.baseName(of: contentId)
        let workerContentId = ProductManifestRecords.subname(base: productId, kind: .worker)

        return contentId == workerContentId || contentId == productId ? productId : nil
    }

    func isPresented(_ contentId: ProductExecutable.Identifier) -> Bool {
        !reloaders(of: contentId).isEmpty
    }

    func reloaders(of contentId: ProductExecutable.Identifier) -> [SPAProductReloading] {
        presences.values.filter { $0.contentId == contentId }.compactMap(\.reloader)
    }

    func showNextNotice() {
        guard noticeContentId == nil,
              let contentId = pendingUpdates.keys.first(where: { !dismissedContentIds.contains($0) }),
              let presence = presences.values.first(where: { $0.contentId == contentId }) else {
            return
        }

        noticeContentId = contentId

        noticePresenter.show(
            productId: presence.productId,
            productName: presence.displayName,
            onUpdate: { [weak self] in _ = self?.applyPendingUpdate(of: contentId) },
            onDismiss: { [weak self] in self?.dismissNotice(of: contentId) }
        )
    }

    func dismissNotice(of contentId: ProductExecutable.Identifier) {
        logger.debug("Product update dismissed for \(contentId)")
        dismissedContentIds.insert(contentId)
        hideNotice(of: contentId)
        showNextNotice()
    }

    func hideNotice(of contentId: ProductExecutable.Identifier) {
        guard noticeContentId == contentId else { return }

        noticeContentId = nil
        noticePresenter.hide()
    }

    func removeSupersededContent() async {
        let servedHashes = presences.values.compactMap { $0.reloader?.servedContentHash }
        let pendingHashes = pendingUpdates.values.compactMap { $0.archive?.contentHash }

        await contentLoader.removeSupersededContent(keeping: Set(servedHashes + pendingHashes))
    }

    func refreshProducts() async {
        let presented = Set(presences.values.map(\.productId))
        let hostPlaced = await refreshSources.hostPlacedProducts.placedProductIds()

        presented.union(hostPlaced).forEach(requestRefresh(of:))
    }
}
