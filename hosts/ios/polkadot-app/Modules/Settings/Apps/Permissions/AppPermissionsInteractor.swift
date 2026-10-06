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
    private var mediaSubscriptionTask: Task<Void, Never>?
    private var mediaMutationTask: Task<Void, Never>?

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
        mediaSubscriptionTask?.cancel()
        mediaMutationTask?.cancel()
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
                    let legacy = grants.filter { grant in
                        switch grant.permission {
                        case .deviceCapability(.camera), .deviceCapability(.microphone): return false
                        default: return true
                        }
                    }
                    await self?.presenter?.didReceive(grants: legacy)
                }
            } catch {
                logger.error("App permissions subscription error: \(error)")
            }
        }
        mediaSubscriptionTask = Task { [weak self, productId] in
            let updates = NotificationCenter.default.notifications(named: TrUAPIMediaPermissionSettings.didChange)
            await self?.refreshMediaPermissions()
            for await update in updates {
                guard !Task.isCancelled else { return }
                if update.userInfo?["productId"] as? String == productId {
                    await self?.refreshMediaPermissions()
                }
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

    func setMediaPermission(_ setting: TrUAPIMediaPermissionSetting, allowed: Bool) {
        mediaMutationTask?.cancel()
        mediaMutationTask = Task { [weak self, productId, logger] in
            do {
                try await TrUAPIMediaPermissionSettings(productId: productId).set(setting, allowed: allowed)
            } catch {
                logger.warning("Media permission change was not applied")
            }
            await self?.refreshMediaPermissions()
        }
    }

    private func refreshMediaPermissions() async {
        do {
            let settings = try await TrUAPIMediaPermissionSettings(productId: productId).snapshot()
            await presenter?.didReceive(mediaPermissions: settings)
        } catch {
            logger.warning("Media permissions are unavailable")
            await presenter?.didReceive(mediaPermissions: [])
        }
    }
}
