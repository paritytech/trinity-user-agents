import Foundation
import Products
import TrUAPIHost

@MainActor
final class AppPermissionsInteractor {
    weak var presenter: AppPermissionsInteractorOutputProtocol?

    private let productId: ProductId
    private let providerFactory: ProductPermissionDataProviderMaking
    private let repository: ProductPermissionRepositoryProtocol
    private let notificationScheduler: ProductNotificationScheduling
    private let logger: LoggerProtocol
    private let runtimeProvider: TrUAPIHostRuntimeProviding?

    private var subscriptionTask: Task<Void, Never>?
    private var mediaSubscriptionTask: Task<Void, Never>?
    private var permissionChangesTask: Task<Void, Never>?
    private var mediaMutationTask: Task<Void, Never>?
    private var authorizationTask: Task<Void, Never>?
    private var authorizationReadTask: Task<Void, Never>?
    private var authorizationWriteTask: Task<Void, Never>?

    init(
        productId: ProductId,
        providerFactory: ProductPermissionDataProviderMaking,
        repository: ProductPermissionRepositoryProtocol,
        runtimeProvider: TrUAPIHostRuntimeProviding?,
        notificationScheduler: ProductNotificationScheduling = ProductNotificationScheduler.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productId = productId
        self.providerFactory = providerFactory
        self.repository = repository
        self.runtimeProvider = runtimeProvider
        self.notificationScheduler = notificationScheduler
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
        mediaSubscriptionTask?.cancel()
        permissionChangesTask?.cancel()
        authorizationTask?.cancel()
        authorizationReadTask?.cancel()
        // An explicit settings write must finish even if the user leaves this screen.
    }
}

extension AppPermissionsInteractor: AppPermissionsInteractorInputProtocol {
    func setup() {
        if let runtimeProvider {
            let scopes = runtimeProvider.observeAuthorizationScope()
            authorizationTask = Task { [weak self] in
                for await _ in scopes {
                    guard !Task.isCancelled else { return }
                    self?.refreshAutomaticUploads()
                }
            }
        }
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
                    self?.presenter?.didReceive(grants: legacy)
                }
            } catch {
                logger.error("App permissions subscription error: \(error)")
                self?.presenter?.didReceive(error: error)
            }
        }
        mediaSubscriptionTask = Task { [weak self, productId] in
            let updates = NotificationCenter.default.notifications(named: .productPermissionAuthorizationsChanged)
            await self?.refreshMediaPermissions()
            for await update in updates {
                guard !Task.isCancelled else { return }
                if update.object as? String == productId {
                    await self?.refreshMediaPermissions()
                    self?.refreshAutomaticUploads(invalidate: false)
                }
            }
        }
        permissionChangesTask = Task { [weak self, productId] in
            let updates = NotificationCenter.default.notifications(named: TrUAPIMediaPermissionSettings.didChange)
            for await update in updates {
                guard !Task.isCancelled else { return }
                if update.userInfo?["productId"] as? String == productId {
                    await self?.refreshMediaPermissions()
                }
            }
        }
    }

    func setAutomaticUploads(allowed: Bool, scope: TrUAPIAutomaticUploadScope) {
        guard let runtimeProvider, authorizationWriteTask == nil else {
            presenter?.didFinishRevoking()
            return
        }
        authorizationReadTask?.cancel()
        authorizationWriteTask = Task { [weak self, productId, logger] in
            do {
                guard try runtimeProvider.automaticUploadScope() == scope else {
                    self?.finishAutomaticUploadWrite()
                    return
                }
                let runtime = try runtimeProvider.sharedRuntime()
                try await runtime.setPermissionAuthorizationStatus(
                    productId: productId,
                    request: .automaticPreimageSubmit(rootPublicKey: scope.rootPublicKey),
                    status: allowed ? .authorized : .notDetermined
                )
            } catch {
                logger.error("Failed to update automatic upload consent: \(error)")
                self?.presenter?.didReceive(error: error)
            }
            self?.finishAutomaticUploadWrite()
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
                presenter?.didReceive(grants: grants.filter {
                    guard $0.granted else { return false }
                    switch $0.permission {
                    case .deviceCapability(.camera), .deviceCapability(.microphone): return false
                    default: return true
                    }
                })
                presenter?.didFinishRevoking()
            } catch {
                logger.error("Failed to revoke product permissions: \(error)")
                presenter?.didFinishRevoking()
                presenter?.didReceive(error: error)
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
                self?.presenter?.didReceive(error: error)
            }
            await self?.refreshMediaPermissions()
            self?.presenter?.didFinishRevoking()
        }
    }

    private func refreshMediaPermissions() async {
        do {
            let settings = try await TrUAPIMediaPermissionSettings(productId: productId).snapshot()
            presenter?.didReceive(mediaPermissions: settings)
        } catch {
            logger.warning("Media permissions are unavailable")
            presenter?.didReceive(error: error)
        }
    }
}

private extension AppPermissionsInteractor {
    func finishAutomaticUploadWrite() {
        authorizationWriteTask = nil
        refreshAutomaticUploads(invalidate: false)
        presenter?.didFinishRevoking()
    }

    func refreshAutomaticUploads(invalidate: Bool = true) {
        authorizationReadTask?.cancel()
        if invalidate { presenter?.didReceiveAutomaticUploads(scope: nil, allowed: false) }
        guard let runtimeProvider else { return }
        authorizationReadTask = Task { [weak self, productId, logger] in
            do {
                guard let scope = try runtimeProvider.automaticUploadScope() else {
                    self?.presenter?.didReceiveAutomaticUploads(scope: nil, allowed: false)
                    return
                }
                let runtime = try runtimeProvider.sharedRuntime()
                let status = try await runtime.permissionAuthorizationStatus(
                    productId: productId,
                    request: .automaticPreimageSubmit(rootPublicKey: scope.rootPublicKey)
                )
                // An old response or a lock/account transition cannot repopulate this row.
                guard !Task.isCancelled,
                      try runtimeProvider.automaticUploadScope() == scope else { return }
                self?.presenter?.didReceiveAutomaticUploads(scope: scope, allowed: status == .authorized)
            } catch {
                logger.error("Failed to read automatic upload consent: \(error)")
                self?.presenter?.didReceive(error: error)
            }
        }
    }
}
