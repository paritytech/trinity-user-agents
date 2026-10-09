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
                await self?.presenter?.didReceive(error: error)
            }
        }
    }

    func revoke(permissions: [ProductPermission]) {
        guard !permissions.isEmpty else { return }
        let revokesNotifications = permissions.contains(.deviceCapability(.notifications))

        Task { [self] in
            do {
                // Complete native cleanup before publishing the revoked snapshot.
                // A cancellation failure leaves the row available to retry.
                if revokesNotifications {
                    try await notificationScheduler.cancelAll(forProductId: productId)
                }
                try await repository.revoke(productId: productId, permissions: permissions)
                let grants = try await repository.getAllByProduct(productId: productId)
                await presenter?.didReceive(grants: grants.filter(\.granted))
                await presenter?.didFinishRevoking()
            } catch {
                logger.error("Failed to revoke product permissions: \(error)")
                await presenter?.didFinishRevoking()
                await presenter?.didReceive(error: error)
            }
        }
    }
}
