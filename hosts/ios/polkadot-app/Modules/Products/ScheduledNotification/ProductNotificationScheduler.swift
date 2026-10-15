import Foundation
import Operation_iOS
import Products
import UserNotifications

protocol ProductNotificationScheduling {
    func schedule(
        productId: ProductId,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32

    func cancel(productId: ProductId, notificationId: UInt32) async throws

    func cancelAll(forProductId productId: ProductId) async throws
}

actor ProductNotificationScheduler: ProductNotificationScheduling {
    static let shared = ProductNotificationScheduler()

    private static let globalSlotLimit = 64

    private let storageFacade: StorageFacadeProtocol
    private let notificationCenter: UNUserNotificationCenter
    private let mapper: AnyCoreDataMapper<ScheduledNotificationEntry, CDScheduledNotification>
    private let userStates: ChatRoomUserStateReading
    private let notificationAdding: (UNNotificationRequest) async throws -> Void
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        notificationCenter: UNUserNotificationCenter = .current(),
        userStates: ChatRoomUserStateReading? = nil,
        notificationAdding: ((UNNotificationRequest) async throws -> Void)? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.notificationCenter = notificationCenter
        mapper = AnyCoreDataMapper(ScheduledNotificationMapper())
        self.userStates = userStates ?? ChatRoomUserStateReader(storageFacade: storageFacade)
        self.notificationAdding = notificationAdding ?? Self.systemAdding(notificationCenter, logger: logger)
        self.logger = logger
    }

    func schedule(
        productId: ProductId,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32 {
        if await opensMutedRoom(productId: productId, request: request) {
            logger.debug("Notification of \(productId) into a muted room shows nothing")
            return Self.hashId(productId: productId)
        }

        try await reconcileStaleEntries()

        let allRepository = createRepository()
        let currentCount = try await allRepository.fetchAllOperation(with: .init()).asyncExecute().count
        guard currentCount < Self.globalSlotLimit else {
            throw ProductNativeApiError.scheduleLimitReached
        }

        let notificationId = try await generateUniqueId(productId: productId)
        let entry = ScheduledNotificationEntry(
            productId: productId,
            notificationId: notificationId
        )

        let content = UNMutableNotificationContent()
        content.title = productId
        content.body = request.text
        content.sound = .default

        var userInfo: [String: Any] = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue
        ]

        if let deeplink = request.deeplink {
            userInfo[PushNotificationKeys.deeplink] = deeplink
        }

        content.userInfo = userInfo

        let trigger = makeTrigger(scheduledAtMs: request.scheduledAtMs)

        let notificationRequest = UNNotificationRequest(
            identifier: entry.unIdentifier,
            content: content,
            trigger: trigger
        )

        try await notificationAdding(notificationRequest)
        try await allRepository.saveOperation({ [entry] }, { [] }).asyncExecute()

        logger.debug("Scheduled push notification for \(productId)")

        return notificationId
    }

    func cancel(productId: ProductId, notificationId: UInt32) async throws {
        let entry = ScheduledNotificationEntry(
            productId: productId,
            notificationId: notificationId
        )

        notificationCenter.removePendingNotificationRequests(
            withIdentifiers: [entry.unIdentifier]
        )

        let repository = createRepository()
        try await repository.saveOperation({ [] }, { [entry.identifier] }).asyncExecute()

        logger.debug("Cancelled notification \(notificationId) \(productId)")
    }

    func cancelAll(forProductId productId: ProductId) async throws {
        let repository = createProductRepository(productId: productId)

        let entries = try await repository.fetchAllOperation(with: .init()).asyncExecute()
        guard !entries.isEmpty else { return }

        let unIdentifiers = entries.map(\.unIdentifier)
        notificationCenter.removePendingNotificationRequests(withIdentifiers: unIdentifiers)

        try await repository.deleteAllOperation().asyncExecute()

        logger.debug("Cancelled notifications \(productId)")
    }
}

// MARK: - Private

private extension ProductNotificationScheduler {
    /// Gives a notification to the OS. Under the truapi E2E harness that grants notifications, the app has no OS
    /// permission and the OS never answers, so the harness gets a log line in place of the notification.
    static func systemAdding(
        _ notificationCenter: UNUserNotificationCenter,
        logger: LoggerProtocol
    ) -> (UNNotificationRequest) async throws -> Void {
        guard !TrUAPIE2ENotificationGrant.isOn else {
            return { logger.debug("[e2e] notification \($0.identifier) given to the OS") }
        }
        return { try await notificationCenter.add($0) }
    }

    /// True when the deeplink of the notification opens a room of the product that the user muted until after the
    /// notification shows (D34). The OS shows a scheduled notification, so a mute that starts after the schedule
    /// does not stop it.
    func opensMutedRoom(productId: ProductId, request: ScheduledNotificationRequest) async -> Bool {
        guard
            let deeplink = request.deeplink,
            let link = ChatRoomLink(deeplink),
            link.productId == productId
        else {
            return false
        }

        let showsAt = request.scheduledAtMs.map { Date.fromChatTimestamp($0) } ?? Date()
        do {
            return try await userStates.state(of: link.chatId).isMuted(at: max(showsAt, Date()))
        } catch {
            logger.warning("Notification of \(productId) shows: the room state is not readable: \(error)")
            return false
        }
    }

    func createRepository() -> AnyDataProviderRepository<ScheduledNotificationEntry> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: mapper
            )
        )
    }

    func createProductRepository(
        productId: ProductId
    ) -> AnyDataProviderRepository<ScheduledNotificationEntry> {
        let filter = NSPredicate(
            format: "%K == %@",
            #keyPath(CDScheduledNotification.productId),
            productId
        )

        return AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: mapper
            )
        )
    }

    func reconcileStaleEntries() async throws {
        let repository = createRepository()
        let dbEntries = try await repository.fetchAllOperation(with: .init()).asyncExecute()
        guard !dbEntries.isEmpty else { return }

        let pendingRequests = await notificationCenter.pendingNotificationRequests()
        let pendingIdentifiers = Set(pendingRequests.map(\.identifier))

        let staleIdentifiers = dbEntries
            .filter { !pendingIdentifiers.contains($0.unIdentifier) }
            .map(\.identifier)

        guard !staleIdentifiers.isEmpty else { return }

        try await repository.saveOperation({ [] }, { staleIdentifiers }).asyncExecute()

        logger.debug("Pruned \(staleIdentifiers.count) stale scheduled notification entries")
    }

    func generateUniqueId(productId: ProductId) async throws -> UInt32 {
        let repository = createProductRepository(productId: productId)
        let existingIds = try await Set(
            repository.fetchAllOperation(with: .init())
                .asyncExecute()
                .map(\.notificationId)
        )

        var candidate = Self.hashId(productId: productId)

        while existingIds.contains(candidate) {
            candidate &+= 1
        }

        return candidate
    }

    static func hashId(productId: ProductId) -> UInt32 {
        var hasher = Hasher()
        hasher.combine(productId)
        hasher.combine(Date().timeIntervalSince1970)
        return UInt32(truncatingIfNeeded: hasher.finalize())
    }

    func makeTrigger(scheduledAtMs: UInt64?) -> UNNotificationTrigger {
        guard let scheduledAtMs, scheduledAtMs > 0 else {
            return UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false)
        }

        let nowMs = UInt64(Date().timeIntervalSince1970.milliseconds)

        guard scheduledAtMs > nowMs else {
            return UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false)
        }

        let deltaSeconds = TimeInterval(scheduledAtMs - nowMs).seconds
        return UNTimeIntervalNotificationTrigger(timeInterval: deltaSeconds, repeats: false)
    }
}
