import Foundation
import Products

final class AppPermissionsInteractor {
    weak var presenter: AppPermissionsInteractorOutputProtocol?

    private let productId: ProductId
    private let providerFactory: ProductPermissionDataProviderMaking
    private let repository: ProductPermissionRepositoryProtocol
    private let coreAuthorizations: TrUAPIPermissionAuthorizing?
    private let notificationScheduler: ProductNotificationScheduling
    private let logger: LoggerProtocol

    private var nativeGrantsTask: Task<Void, Never>?
    private var coreGrantsTask: Task<Void, Never>?

    @MainActor private var nativeGrants: [ProductPermissionGrant] = []
    @MainActor private var coreGrants: [ProductPermissionGrant] = []

    init(
        productId: ProductId,
        providerFactory: ProductPermissionDataProviderMaking,
        repository: ProductPermissionRepositoryProtocol,
        coreAuthorizations: TrUAPIPermissionAuthorizing?,
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.providerFactory = providerFactory
        self.repository = repository
        self.coreAuthorizations = coreAuthorizations
        self.notificationScheduler = notificationScheduler
        self.logger = logger
    }

    deinit {
        nativeGrantsTask?.cancel()
        coreGrantsTask?.cancel()
    }
}

extension AppPermissionsInteractor: AppPermissionsInteractorInputProtocol {
    func setup() {
        nativeGrantsTask = Task { [weak self, providerFactory, productId, logger] in
            do {
                for try await grants in providerFactory.subscribeGrants(productId: productId, grantedOnly: true) {
                    await self?.handle(nativeGrants: grants)
                }
            } catch {
                logger.error("App permissions subscription error: \(error)")
            }
        }

        coreGrantsTask = Task { [weak self, productId, logger] in
            // Subscribed before the first read, so a change raised during it is
            // buffered rather than lost.
            guard var changes = self?.coreAuthorizations?.changes.makeAsyncIterator() else { return }

            await self?.reloadCoreGrants()

            do {
                while let changedProductId = try await changes.next() {
                    guard changedProductId == productId else { continue }

                    await self?.reloadCoreGrants()
                }
            } catch {
                logger.error("Core permission changes subscription error: \(error)")
            }
        }
    }

    func revokeOnDisappear(permissions: [ProductPermission]) {
        guard !permissions.isEmpty else { return }

        // currently revoke is called once, when scene closed
        // stop subscription to not update UI during disappear
        nativeGrantsTask?.cancel()
        coreGrantsTask?.cancel()

        let revokesNotifications = permissions.contains(.deviceCapability(.notifications))

        Task { [repository, coreAuthorizations, notificationScheduler, productId, logger] in
            do {
                try await repository.revoke(productId: productId, permissions: permissions)

                if revokesNotifications {
                    try await notificationScheduler.cancelAll(forProductId: productId)
                }
            } catch {
                logger.error("Failed to revoke product permissions: \(error)")
            }

            do {
                try coreAuthorizations?.reset(productId: productId, permissions: permissions)
            } catch {
                logger.error("Failed to reset product permissions in the core: \(error)")
            }
        }
    }
}

private extension AppPermissionsInteractor {
    @MainActor
    func reloadCoreGrants() async {
        guard let coreAuthorizations else { return }

        do {
            let grants = try await coreAuthorizations.grants()
            handle(coreGrants: grants.filter { $0.productId == productId })
        } catch {
            logger.error("Failed to list the core's permission grants: \(error)")
        }
    }

    @MainActor
    func handle(nativeGrants: [ProductPermissionGrant]) {
        self.nativeGrants = nativeGrants
        publish()
    }

    @MainActor
    func handle(coreGrants: [ProductPermissionGrant]) {
        self.coreGrants = coreGrants
        publish()
    }

    /// One row per permission whichever store holds it. The native store lists
    /// in type and key order; the core lists in storage order, so its grants
    /// are sorted the same way before they follow.
    @MainActor
    func publish() {
        var seen = Set<String>()
        let coreOnly = coreGrants.sorted {
            ($0.permission.typeName, $0.permission.key) < ($1.permission.typeName, $1.permission.key)
        }
        let grants = (nativeGrants + coreOnly).filter { seen.insert($0.identifier).inserted }

        presenter?.didReceive(grants: grants)
    }
}
