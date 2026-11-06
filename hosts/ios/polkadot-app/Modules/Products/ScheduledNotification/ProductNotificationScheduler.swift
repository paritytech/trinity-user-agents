import struct TrUAPIHost.ProductId
import Foundation
import FoundationExt
import IssueMonitoring
import Operation_iOS
import Products
import StructuredConcurrency
import UserNotifications

protocol ProductNotificationScheduling {
    func schedule(
        productId: ProductId,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32

    func cancel(productId: ProductId, notificationId: UInt32) async throws

    func cancelAll(forProductId productId: ProductId) async throws
}

/// The part of the system notification center the scheduler uses.
protocol ScheduledNotificationCentering: Sendable {
    func add(_ request: UNNotificationRequest) async throws

    func pendingNotificationRequests() async -> [UNNotificationRequest]

    func deliveredNotifications() async -> [UNNotification]

    func removePendingNotificationRequests(withIdentifiers identifiers: [String])
}

extension UNUserNotificationCenter: ScheduledNotificationCentering {}

actor ProductNotificationScheduler: ProductNotificationScheduling {
    static let shared = ProductNotificationScheduler()

    private static let globalSlotLimit = 64

    private let storageFacade: StorageFacadeProtocol
    private let notificationCenter: any ScheduledNotificationCentering
    private let mapper: AnyCoreDataMapper<ScheduledNotificationEntry, CDScheduledNotification>
    private let diagnostics: ProductNotificationDiagnostics
    private let applicationStateStreams: ApplicationStateStreamFactory
    private let logger: LoggerProtocol

    // Actor methods interleave at each await, so a sweep could report a booking twice or call a cancelled one gone.
    private let operations = SerialOperationQueue()
    private var becameActive: Task<Void, Never>?

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        notificationCenter: any ScheduledNotificationCentering = UNUserNotificationCenter.current(),
        diagnostics: ProductNotificationDiagnostics = ProductNotificationDiagnostics(
            recorder: IssueMonitoringFactory.createHostEventRecorder()
        ),
        applicationStateStreams: ApplicationStateStreamFactory = ApplicationStateStreamFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.notificationCenter = notificationCenter
        mapper = AnyCoreDataMapper(ScheduledNotificationMapper())
        self.diagnostics = diagnostics
        self.applicationStateStreams = applicationStateStreams
        self.logger = logger
    }

    /// Sweeps now and at every activation, since a launch passes its first activation before the services start.
    func start() {
        guard becameActive == nil else {
            return
        }
        let events = applicationStateStreams.stream(for: .didBecomeActive)
        becameActive = Task { [weak self] in
            await self?.sweepInTurn()
            for await _ in events {
                guard let self else {
                    return
                }
                await sweepInTurn()
            }
        }
    }

    func schedule(
        productId: ProductId,
        request: ScheduledNotificationRequest
    ) async throws -> UInt32 {
        let notificationId = try await operations.run { [self] in
            try await book(productId: productId, request: request)
        }
        guard Task.isCancelled else {
            return notificationId
        }

        // The queue runs the booking on its own task, so cancelling the caller cannot stop it.
        try await cancel(productId: productId, notificationId: notificationId)
        throw CancellationError()
    }

    func cancel(productId: ProductId, notificationId: UInt32) async throws {
        try await operations.run { [self] in
            try await cancelBooking(productId: productId, notificationId: notificationId)
        }
    }

    func cancelAll(forProductId productId: ProductId) async throws {
        try await operations.run { [self] in
            try await cancelBookings(productId: productId)
        }
    }
}

// MARK: - Private

private extension ProductNotificationScheduler {
    func book(productId: ProductId, request: ScheduledNotificationRequest) async throws -> UInt32 {
        let repository = createRepository()
        let (currentCount, notificationId) = try await reporting(.storeFailed, productId: productId) {
            try await sweep()
            let bookedCount = try await repository.fetchAllOperation(with: .init()).asyncExecute().count
            return try await (bookedCount, generateUniqueId(productId: productId))
        }

        guard currentCount < Self.globalSlotLimit else {
            diagnostics.notScheduled(productId: productId, notificationId: nil, reason: .limitReached)
            throw ProductNativeApiError.scheduleLimitReached
        }

        let entry = ScheduledNotificationEntry(
            productId: productId,
            notificationId: notificationId
        )

        try await reporting(.addFailed, productId: productId, notificationId: notificationId) {
            try await notificationCenter.add(makeRequest(entry: entry, request: request))
        }
        try await reporting(.storeFailed, productId: productId, notificationId: notificationId) {
            try await store(entry, in: repository)
        }

        diagnostics.scheduled(entry: entry, scheduledAtMs: request.scheduledAtMs)
        logger.debug("Scheduled push notification for \(productId)")

        return notificationId
    }

    func reporting<T>(
        _ reason: ProductNotificationEvent.Reason,
        productId: ProductId,
        notificationId: UInt32? = nil,
        _ step: () async throws -> T
    ) async throws -> T {
        do {
            return try await step()
        } catch {
            diagnostics.notScheduled(productId: productId, notificationId: notificationId, reason: reason)
            throw error
        }
    }

    func store(
        _ entry: ScheduledNotificationEntry,
        in repository: AnyDataProviderRepository<ScheduledNotificationEntry>
    ) async throws {
        do {
            try await repository.saveOperation({ [entry] }, { [] }).asyncExecute()
        } catch {
            notificationCenter.removePendingNotificationRequests(withIdentifiers: [entry.unIdentifier])
            throw error
        }
    }

    func makeRequest(
        entry: ScheduledNotificationEntry,
        request: ScheduledNotificationRequest
    ) -> UNNotificationRequest {
        let content = UNMutableNotificationContent()
        content.title = entry.productId.rawValue
        content.body = request.text
        content.sound = .default

        var userInfo: [String: Any] = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue
        ]

        if let deeplink = request.deeplink {
            userInfo[PushNotificationKeys.deeplink] = deeplink
        }

        content.userInfo = userInfo

        return UNNotificationRequest(
            identifier: entry.unIdentifier,
            content: content,
            trigger: makeTrigger(scheduledAtMs: request.scheduledAtMs)
        )
    }

    func cancelBooking(productId: ProductId, notificationId: UInt32) async throws {
        await sweepLoggingFailure()

        let entry = ScheduledNotificationEntry(
            productId: productId,
            notificationId: notificationId
        )

        notificationCenter.removePendingNotificationRequests(
            withIdentifiers: [entry.unIdentifier]
        )

        let repository = createRepository()
        let storedEntry = try await repository.fetchOperation(by: { entry.identifier }, options: .init())
            .asyncExecute()
        guard storedEntry != nil else { return }

        diagnostics.cancelled(entry: entry)

        try await repository.saveOperation({ [] }, { [entry.identifier] }).asyncExecute()

        logger.debug("Cancelled notification \(notificationId) \(productId)")
    }

    func cancelBookings(productId: ProductId) async throws {
        await sweepLoggingFailure()

        let repository = createProductRepository(productId: productId)

        let entries = try await repository.fetchAllOperation(with: .init()).asyncExecute()
        guard !entries.isEmpty else { return }

        let unIdentifiers = entries.map(\.unIdentifier)
        notificationCenter.removePendingNotificationRequests(withIdentifiers: unIdentifiers)
        entries.forEach(diagnostics.cancelled(entry:))

        try await repository.deleteAllOperation().asyncExecute()

        logger.debug("Cancelled notifications \(productId)")
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
            productId.rawValue
        )

        return AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: mapper
            )
        )
    }

    func sweepInTurn() async {
        await operations.run { [self] in
            await sweepLoggingFailure()
        }
    }

    func sweepLoggingFailure() async {
        do {
            try await sweep()
        } catch {
            logger.warning("Scheduled notification sweep failed: \(error)")
        }
    }

    func sweep() async throws {
        let repository = createRepository()
        let dbEntries = try await repository.fetchAllOperation(with: .init()).asyncExecute()
        guard !dbEntries.isEmpty else { return }

        let pendingRequests = await notificationCenter.pendingNotificationRequests()
        let pendingIdentifiers = Set(pendingRequests.map(\.identifier))

        let staleEntries = dbEntries.filter { !pendingIdentifiers.contains($0.unIdentifier) }

        guard !staleEntries.isEmpty else { return }

        let deliveredNotifications = await notificationCenter.deliveredNotifications()
        let deliveredDates = Dictionary(
            deliveredNotifications.map { ($0.request.identifier, $0.date) },
            uniquingKeysWith: { first, _ in first }
        )

        // Pruning first means a failed prune retries without reporting an entry twice.
        try await repository.saveOperation({ [] }, { staleEntries.map(\.identifier) }).asyncExecute()

        for entry in staleEntries {
            if let deliveredAt = deliveredDates[entry.unIdentifier] {
                diagnostics.delivered(entry: entry, deliveredAt: deliveredAt)
            } else {
                diagnostics.gone(entry: entry)
            }
        }

        logger.debug("Pruned \(staleEntries.count) stale scheduled notification entries")
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
