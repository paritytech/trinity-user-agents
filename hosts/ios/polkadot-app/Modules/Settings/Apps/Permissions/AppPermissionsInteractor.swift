import Foundation
import Products

final class AppPermissionsInteractor {
    weak var presenter: AppPermissionsInteractorOutputProtocol?

    private let productId: ProductId
    private let permissions: AppPermissionSettings
    private let notificationScheduler: ProductNotificationScheduling
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?

    init(
        productId: ProductId,
        permissions: AppPermissionSettings,
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.permissions = permissions
        self.notificationScheduler = notificationScheduler
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }
}

extension AppPermissionsInteractor: AppPermissionsInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, permissions, productId, logger] in
            do {
                let stream = try await permissions.grantedRecords(productId: productId)
                for try await grants in stream {
                    await self?.presenter?.didReceive(grants: grants)
                }
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("App permissions subscription error: \(error)")
                await self?.presenter?.didReceive(error: error)
            }
        }
    }

    func revokeOnDisappear(records: [AppPermissionRecord]) {
        guard !records.isEmpty else { return }

        // currently revoke is called once, when scene closed
        // stop subscription to not update UI during disappear
        subscriptionTask?.cancel()

        let revokesNotifications = records.contains(where: \.isNotifications)

        Task { [weak self, permissions, notificationScheduler, productId, logger] in
            do {
                try await permissions.revoke(records)

                if revokesNotifications {
                    try await notificationScheduler.cancelAll(forProductId: productId)
                }
            } catch {
                logger.error("Failed to revoke product permissions: \(error)")
                await self?.presenter?.didReceive(error: error)
            }
        }
    }
}
