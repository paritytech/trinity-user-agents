import Foundation
import FoundationExt
import Products

@MainActor
protocol SPAProductReloading: AnyObject {
    func serveAppliedBuild()
}

protocol ProductWorkerReloading {
    func reloadWorker(of productId: ProductId) async
}

@MainActor
protocol ProductPresenceRegistering: AnyObject {
    func register(productId: ProductId, reloader: SPAProductReloading) -> UUID
    func unregister(_ presenceId: UUID)
    func requestRefresh(of productId: ProductId)
    func applyPendingUpdate(of productId: ProductId) -> Bool
}

@MainActor
final class ProductUpdateCoordinator {
    private struct Presence {
        let productId: ProductId
        weak var reloader: SPAProductReloading?
    }

    private let contentLoader: ProductContentLoading
    private let workerReloader: ProductWorkerReloading
    private let noticePresenter: ProductUpdateNoticePresenting
    private let applicationStateStreams: ApplicationStateStreamFactory
    private let logger: LoggerProtocol

    private var presences: [UUID: Presence] = [:]
    private var pendingUpdates: [ProductId: ProductUpdate] = [:]
    private var dismissedProductIds: Set<ProductId> = []
    private var noticeProductId: ProductId?
    private var updatesTask: Task<Void, Never>?
    private var foregroundTask: Task<Void, Never>?

    init(
        contentLoader: ProductContentLoading,
        workerReloader: ProductWorkerReloading,
        noticePresenter: ProductUpdateNoticePresenting,
        applicationStateStreams: ApplicationStateStreamFactory,
        logger: LoggerProtocol
    ) {
        self.contentLoader = contentLoader
        self.workerReloader = workerReloader
        self.noticePresenter = noticePresenter
        self.applicationStateStreams = applicationStateStreams
        self.logger = logger
    }

    deinit {
        updatesTask?.cancel()
        foregroundTask?.cancel()
    }

    func start() {
        guard updatesTask == nil else { return }

        let updates = contentLoader.updates()
        updatesTask = Task { [weak self, contentLoader] in
            for update in await contentLoader.pendingUpdates() {
                self?.handle(update)
            }

            do {
                for try await update in updates {
                    self?.handle(update)
                }
            } catch {
                self?.logger.error("Product updates stream ended: \(error)")
            }
        }

        let foregroundEvents = applicationStateStreams.stream(for: .willEnterForeground)
        foregroundTask = Task { [weak self] in
            for await _ in foregroundEvents {
                self?.refreshPresentedProducts()
            }
        }
    }
}

extension ProductUpdateCoordinator: ProductPresenceRegistering {
    func register(productId: ProductId, reloader: SPAProductReloading) -> UUID {
        let presenceId = UUID()
        presences[presenceId] = Presence(productId: productId.lowercased(), reloader: reloader)
        return presenceId
    }

    func unregister(_ presenceId: UUID) {
        guard let presence = presences.removeValue(forKey: presenceId),
              !isPresented(presence.productId),
              let update = pendingUpdates.removeValue(forKey: presence.productId) else {
            return
        }

        dismissedProductIds.remove(presence.productId)
        hideNotice(of: presence.productId)
        apply(update, reloadingPresentations: false)
    }

    func requestRefresh(of productId: ProductId) {
        Task { [contentLoader] in
            _ = await contentLoader.refresh(productId)
        }
    }

    func applyPendingUpdate(of productId: ProductId) -> Bool {
        let key = productId.lowercased()

        guard let update = pendingUpdates.removeValue(forKey: key) else {
            return false
        }

        dismissedProductIds.remove(key)
        hideNotice(of: key)
        apply(update, reloadingPresentations: true)

        return true
    }
}

private extension ProductUpdateCoordinator {
    func handle(_ update: ProductUpdate) {
        let key = update.productId.lowercased()

        guard isPresented(key) else {
            apply(update, reloadingPresentations: false)
            return
        }

        pendingUpdates[key] = update
        dismissedProductIds.remove(key)
        showNextNotice()
    }

    func apply(_ update: ProductUpdate, reloadingPresentations: Bool) {
        let key = update.productId.lowercased()

        Task { [weak self, contentLoader, workerReloader] in
            await contentLoader.apply(update)

            if reloadingPresentations {
                self?.reloaders(of: key).forEach { $0.serveAppliedBuild() }
            }

            await workerReloader.reloadWorker(of: update.productId)
        }

        showNextNotice()
    }

    func isPresented(_ productId: ProductId) -> Bool {
        !reloaders(of: productId).isEmpty
    }

    func reloaders(of productId: ProductId) -> [SPAProductReloading] {
        presences.values.filter { $0.productId == productId }.compactMap(\.reloader)
    }

    func showNextNotice() {
        guard noticeProductId == nil,
              let next = pendingUpdates.first(where: { !dismissedProductIds.contains($0.key) }) else {
            return
        }

        let productId = next.key
        noticeProductId = productId

        noticePresenter.show(
            productName: next.value.latest.resolved.displayName,
            onUpdate: { [weak self] in _ = self?.applyPendingUpdate(of: productId) },
            onDismiss: { [weak self] in self?.dismissNotice(of: productId) }
        )
    }

    func dismissNotice(of productId: ProductId) {
        dismissedProductIds.insert(productId)
        hideNotice(of: productId)
        showNextNotice()
    }

    func hideNotice(of productId: ProductId) {
        guard noticeProductId == productId else { return }

        noticeProductId = nil
        noticePresenter.hide()
    }

    func refreshPresentedProducts() {
        Set(presences.values.map(\.productId)).forEach(requestRefresh(of:))
    }
}
