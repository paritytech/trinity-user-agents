import Foundation
import FoundationExt
import Products

@MainActor
protocol SPAProductReloading: AnyObject {
    func serveAppliedBuild()
}

@MainActor
protocol ProductPresenceRegistering: AnyObject {
    func register(productId: ProductId, contentId: ProductId, reloader: SPAProductReloading) -> UUID
    func unregister(_ presenceId: UUID)
    func requestRefresh(of productId: ProductId)
    func applyPendingUpdate(of contentId: ProductId) -> Bool
}

@MainActor
final class ProductUpdateCoordinator {
    private struct Presence {
        let productId: ProductId
        let contentId: ProductId
        weak var reloader: SPAProductReloading?
    }

    private let contentLoader: ProductContentLoading
    private let applicationStateStreams: ApplicationStateStreamFactory
    private let logger: LoggerProtocol

    private var presences: [UUID: Presence] = [:]
    private var pendingUpdates: [ProductId: ExecutableRelease] = [:]
    private var updatesTask: Task<Void, Never>?
    private var foregroundTask: Task<Void, Never>?

    init(
        contentLoader: ProductContentLoading,
        applicationStateStreams: ApplicationStateStreamFactory,
        logger: LoggerProtocol
    ) {
        self.contentLoader = contentLoader
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
        updatesTask = Task { [weak self] in
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
    func register(productId: ProductId, contentId: ProductId, reloader: SPAProductReloading) -> UUID {
        let presenceId = UUID()
        presences[presenceId] = Presence(productId: productId, contentId: contentId, reloader: reloader)
        return presenceId
    }

    func unregister(_ presenceId: UUID) {
        guard let presence = presences.removeValue(forKey: presenceId),
              !isPresented(presence.contentId),
              let update = pendingUpdates.removeValue(forKey: presence.contentId) else {
            return
        }

        apply(update, reloadingPresentations: false)
    }

    func requestRefresh(of productId: ProductId) {
        Task { [contentLoader] in
            _ = await contentLoader.refresh(productId)
        }
    }

    func applyPendingUpdate(of contentId: ProductId) -> Bool {
        guard let update = pendingUpdates.removeValue(forKey: contentId) else {
            return false
        }

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
    }

    func apply(_ update: ExecutableRelease, reloadingPresentations: Bool) {
        let contentId = update.contentId

        logger.debug("Applying the update of \(contentId), reloading open SPAs: \(reloadingPresentations)")

        Task { [weak self, contentLoader] in
            await contentLoader.apply(update)

            if reloadingPresentations {
                self?.reloaders(of: contentId).forEach { $0.serveAppliedBuild() }
            }
        }
    }

    func isPresented(_ contentId: ProductId) -> Bool {
        !reloaders(of: contentId).isEmpty
    }

    func reloaders(of contentId: ProductId) -> [SPAProductReloading] {
        presences.values.filter { $0.contentId == contentId }.compactMap(\.reloader)
    }

    func refreshPresentedProducts() {
        Set(presences.values.map(\.productId)).forEach(requestRefresh(of:))
    }
}
