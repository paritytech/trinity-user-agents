import Foundation
import FoundationExt
import Products

@MainActor
protocol SPAProductReloading: AnyObject {
    func serveAppliedBuild()
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
    private let applicationStateStreams: ApplicationStateStreamFactory
    private let logger: LoggerProtocol

    private var presences: [UUID: Presence] = [:]
    private var pendingUpdates: [ProductId: ProductUpdate] = [:]
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
    }

    func apply(_ update: ProductUpdate, reloadingPresentations: Bool) {
        let key = update.productId.lowercased()

        logger.debug("Applying product update for \(key), reloading open SPAs: \(reloadingPresentations)")

        Task { [weak self, contentLoader] in
            await contentLoader.apply(update)

            if reloadingPresentations {
                self?.reloaders(of: key).forEach { $0.serveAppliedBuild() }
            }
        }
    }

    func isPresented(_ productId: ProductId) -> Bool {
        !reloaders(of: productId).isEmpty
    }

    func reloaders(of productId: ProductId) -> [SPAProductReloading] {
        presences.values.filter { $0.productId == productId }.compactMap(\.reloader)
    }

    func refreshPresentedProducts() {
        Set(presences.values.map(\.productId)).forEach(requestRefresh(of:))
    }
}
