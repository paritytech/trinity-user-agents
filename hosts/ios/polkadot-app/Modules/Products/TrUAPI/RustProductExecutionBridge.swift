import Foundation
import FoundationExt
import TrUAPIHost
import Products
import ChainRegistry
import SubstrateSdk

/// Production `HostBridge` for one product execution: wires the rust core's
/// platform callbacks to app services and, once attached, notifies the
/// execution in place. Subclassable so modality-specific bridges (e.g.
/// ``RustChatExecutionBridge``) can layer extra callbacks on top.
/// Threading contract: async callbacks (navigateTo/devicePermission/
/// remotePermission/pushNotification/confirmUserAction/featureSupported/
/// lookupPreimage) are awaited by the core and may present UI; rust dropping
/// the future cancels the Swift task. Sync callbacks run inline on the
/// dispatcher thread and must return promptly.
class RustProductExecutionBridge: HostBridge, @unchecked Sendable {
    struct Dependencies {
        let productId: ProductId
        let permissionGuard: ProductPermissionGuarding
        let osPermissionAsker: OSPermissionAsking
        let notificationScheduler: ProductNotificationScheduling
        let gameReminders: ProductGameReminderScheduling?
        let reminderPermissionAsker: ReminderPermissionAsking
        let navigationRouter: ProductsNavigationRouting
        let chainRegistry: ChainRegistryProtocol
        let chainConnections: TrUAPIChainConnecting
        let productStorage: TrUAPILocalStoring
        let coreStorage: TrUAPILocalStoring
        let confirmationPresenter: TrUAPIConfirmationPresenting
        let preimageCache: TrUAPIPreimageLookuping
        let hostProvider: ProductHostProviding
        let logger: LoggerProtocol
    }

    let storage: HostStorageBackend
    let coreStorage: HostCoreStorageBackend

    private let dependencies: Dependencies
    private weak var execution: TrUAPIProductExecutionProtocol?

    init(dependencies: Dependencies) {
        self.dependencies = dependencies
        storage = ProductStorageBackend(storage: dependencies.productStorage)
        coreStorage = CoreStorageBackend(storage: dependencies.coreStorage)
    }

    /// Attach the opened execution so callbacks can notify it in place.
    func attach(_ execution: TrUAPIProductExecutionProtocol) {
        self.execution = execution
        dependencies.chainConnections.eventHandler = self
    }

    func onCoreLog(marker: String, detail: String) {
        dependencies.logger.debug("[truapi:\(marker)] \(detail)")
    }

    func navigateTo(url: String) async throws {
        if let destination = dependencies.hostProvider.page(navigationDestination: url) {
            try await dependencies.navigationRouter.navigateTo(destination: destination)
        } else if let parsed = URL(string: url) {
            try await dependencies.navigationRouter.openExternalURL(parsed)
        } else {
            throw HostNavigateToError.Unknown(reason: "invalid navigation url")
        }
    }

    func devicePermission(
        product _: ProductExecutionConfig,
        request: HostDevicePermissionRequest
    ) async throws -> TrUAPIPermissionDecision {
        try await dependencies.permissionGuard.requestDevicePermissionDecision(
            productId: dependencies.productId,
            capability: request.deviceCapabilityType
        ).hostDecision
    }

    func devicePermissionStatus(request: HostDevicePermissionRequest) async throws -> DevicePermissionStatus {
        switch request {
        case .camera,
             .microphone,
             .notifications:
            switch await dependencies.osPermissionAsker.checkPermission(for: request.deviceCapabilityType) {
            case .allowed: .granted
            case .denied: .denied
            case .notDetermined: .notDetermined
            }
        case .location:
            .notDetermined
        case .bluetooth,
             .nfc,
             .clipboard,
             .openUrl,
             .biometrics:
            .notApplicable
        }
    }

    func remotePermission(
        product _: ProductExecutionConfig,
        request: RemotePermission
    ) async throws -> TrUAPIPermissionDecision {
        try await dependencies.permissionGuard.requestPermissionsDecision(
            productId: dependencies.productId,
            permissions: request.toDomainRequest().toDomainPermissions()
        ).hostDecision
    }

    func pushNotification(request: HostPushNotificationRequest) async throws -> UInt32 {
        try await dependencies.notificationScheduler.schedule(
            productId: dependencies.productId,
            request: request.toScheduledNotificationRequest()
        )
    }

    func cancelNotification(id: UInt32) throws {
        Task { [dependencies] in
            do {
                try await dependencies.notificationScheduler.cancel(
                    productId: dependencies.productId,
                    notificationId: id
                )
            } catch {
                dependencies.logger.error("[truapi] cancel notification \(id) failed: \(error)")
            }
        }
    }

    func confirmUserAction(review: UserConfirmationReview) async throws -> Bool {
        await dependencies.confirmationPresenter.confirm(review: review, from: dependencies.productId)
    }

    func confirmPermission(review: UserConfirmationReview) async throws -> TrUAPIPermissionDecision {
        await dependencies.confirmationPresenter.confirmPermission(review: review, from: dependencies.productId)
    }

    func chainConnect(genesisHash: Data) throws -> UInt32? {
        dependencies.chainConnections.connect(genesisHash: genesisHash)
    }

    func chainSend(connectionId: UInt32, request: String) throws {
        try dependencies.chainConnections.send(connectionId: connectionId, request: request)
    }

    func chainClose(connectionId: UInt32) throws {
        dependencies.chainConnections.close(connectionId: connectionId)
    }

    func lookupPreimage(key: Data) async throws -> Data? {
        await dependencies.preimageCache.lookup(key: key)
    }

    func currentTheme() throws -> ThemeVariant {
        .dark
    }

    func featureSupported(request: HostFeatureSupportedRequest) async throws -> Bool {
        switch request {
        case let .chain(genesisHash):
            // Aligned with chain_connect: supported means the app holds a
            // live connection, not merely a registry entry.
            guard let chain = dependencies.chainRegistry.getChainByGenesis(
                for: genesisHash.toHex()
            ) else {
                return false
            }
            return dependencies.chainRegistry.getConnection(for: chain.chainId) != nil
        }
    }

    func supportedChains() throws -> HostChainSet {
        TrUAPISupportedChains.make(chainRegistry: dependencies.chainRegistry)
    }

    func authStateChanged(state: AuthState) {
        let details =
            switch state {
            case .disconnected:
                "disconnected"
            case .pairing:
                "pairing"
            case .connected:
                "connected"
            case .loginFailed:
                "loging failed"
            case .authenticating:
                "authenticating"
            }

        dependencies.logger.debug("[truapi] auth state: \(details)")
    }
}

// MARK: - Notify-back event handlers

extension RustProductExecutionBridge: TrUAPIChainEventHandling {
    func chainDidReceiveResponse(connectionId: UInt32, json: String) {
        execution?.notifyChainResponse(connectionId: connectionId, json: json)
    }

    func chainDidClose(connectionId: UInt32) {
        execution?.notifyChainClosed(connectionId: connectionId)
    }
}

// MARK: - Game reminders

extension RustProductExecutionBridge: GameHostBridge {
    func scheduleReminder(startsAt: UInt64) async throws {
        let asker = dependencies.reminderPermissionAsker
        let ringAlarm: Bool
        if await asker.askAlarm() {
            ringAlarm = true
        } else if await asker.askNotifications() {
            ringAlarm = false
        } else {
            throw HostRejection.Rejected(reason: "alarms and notifications are both turned off")
        }
        await dependencies.gameReminders?.schedule(
            productId: dependencies.productId,
            startsAt: Date(timeIntervalSince1970: startsAt.millisecondsToSeconds()),
            ringAlarm: ringAlarm,
            addCalendarEvent: true
        )
    }

    func cancelReminder() async throws {
        await dependencies.gameReminders?.cancel(productId: dependencies.productId)
    }
}

// MARK: - Mappers

extension Products.PermissionDecision {
    var hostDecision: TrUAPIPermissionDecision {
        switch self {
        case .allowOnce: .allowOnce
        case .allowAlways: .allowAlways
        case .deny: .deny
        }
    }
}

extension HostDevicePermissionRequest {
    /// Maps the TrUAPI device permission to the Products domain type.
    var deviceCapabilityType: DeviceCapabilityType {
        switch self {
        case .notifications: .notifications
        case .camera: .camera
        case .microphone: .microphone
        case .bluetooth: .bluetooth
        case .nfc: .nfc
        case .location: .location
        case .clipboard: .clipboard
        case .openUrl: .openUrl
        case .biometrics: .biometrics
        }
    }
}

extension RemotePermission {
    /// Maps the TrUAPI remote permission to the Products domain request.
    func toDomainRequest() -> Products.RemotePermissionRequest {
        switch self {
        case let .remote(domains): .remote(domains: domains)
        case .webRtc: .webRTC
        case .chainSubmit: .chainSubmit
        case .preimageSubmit: .preimageSubmit
        case .statementSubmit: .statementSubmit
        }
    }
}

extension HostPushNotificationRequest {
    func toScheduledNotificationRequest() -> ScheduledNotificationRequest {
        ScheduledNotificationRequest(text: text, deeplink: deeplink, scheduledAtMs: scheduledAt)
    }
}
