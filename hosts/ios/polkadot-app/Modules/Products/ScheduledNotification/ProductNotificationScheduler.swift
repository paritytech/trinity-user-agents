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
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        notificationCenter: UNUserNotificationCenter = .current(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.notificationCenter = notificationCenter
        mapper = AnyCoreDataMapper(ScheduledNotificationMapper())
        self.logger = logger
    }

    func schedule(
        productId: ProductId,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32 {
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

        try await notificationCenter.add(notificationRequest)
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
