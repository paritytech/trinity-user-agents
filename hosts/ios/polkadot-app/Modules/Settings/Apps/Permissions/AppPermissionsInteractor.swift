import Foundation
import Products

final class AppPermissionsInteractor {
    weak var presenter: AppPermissionsInteractorOutputProtocol?

    private let productId: ProductId
    private let providerFactory: ProductPermissionDataProviderMaking
    private let repository: ProductPermissionRepositoryProtocol
    private let notificationScheduler: ProductNotificationScheduling
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?

    init(
        productId: ProductId,
        providerFactory: ProductPermissionDataProviderMaking,
        repository: ProductPermissionRepositoryProtocol,
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.providerFactory = providerFactory
        self.repository = repository
        self.notificationScheduler = notificationScheduler
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }
}

extension AppPermissionsInteractor: AppPermissionsInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, providerFactory, productId, logger] in
            let stream = providerFactory.subscribeGrants(
                productId: productId,
                grantedOnly: true
            )

            do {
                for try await grants in stream {
                    await self?.presenter?.didReceive(grants: grants)
                }
            } catch {
                logger.error("App permissions subscription error: \(error)")
            }
        }
    }

    func revokeOnDisappear(permissions: [ProductPermission]) {
        guard !permissions.isEmpty else { return }

        // currently revoke is called once, when scene closed
        // stop subscription to not update UI during disappear
        subscriptionTask?.cancel()

        let revokesNotifications = permissions.contains(.deviceCapability(.notifications))

        Task { [repository, notificationScheduler, productId, logger] in
            do {
                try await repository.revoke(productId: productId, permissions: permissions)

                if revokesNotifications {
                    try await notificationScheduler.cancelAll(forProductId: productId)
                }
            } catch {
                logger.error("Failed to revoke product permissions: \(error)")
            }
        }
    }
}
