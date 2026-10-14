import Foundation

/// Manages the application badge count based on total unread messages across all chats.
/// Subscribes to Core Data changes and syncs the system badge.
final class NotificationBadgeSyncService {
    private let notificationService: UserNotificationServicing
    private let unreadMessageCountService: UnreadMessageCountServicing
    private let logger: LoggerProtocol
    private var task: Task<Void, Never>?

    init(
        notificationService: UserNotificationServicing = UserNotificationService.shared,
        unreadMessageCountService: UnreadMessageCountServicing = UnreadMessageCountService(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.notificationService = notificationService
        self.unreadMessageCountService = unreadMessageCountService
        self.logger = logger
    }

    deinit {
        task?.cancel()
    }

    /// Starts syncing badge counts from Core Data.
    func setup() {
        logger.debug("NotificationBadgeSyncService: Starting badge sync")
        subscribeToUnreadCountChanges()
    }

    /// Stops syncing badge counts.
    func throttle() {
        logger.debug("NotificationBadgeSyncService: Throttling badge sync")
        task?.cancel()
    }

    private func subscribeToUnreadCountChanges() {
        task?.cancel()

        task = Task { [weak self] in
            guard let self else { return }

            let countStream = unreadMessageCountService.totalUnreadBadgeMessageCountStream()

            do {
                for try await count in countStream {
                    await syncBadgeCount(count)
                }

                logger.debug("NotificationBadgeSyncService: Unread count stream ended")
            } catch {
                logger.error("NotificationBadgeSyncService: Unexpected error: \(error)")
            }
        }
    }

    private func syncBadgeCount(_ count: Int) async {
        await notificationService.setBadge(count)

        logger.debug("NotificationBadgeSyncService: Synced badge count to \(count)")
    }
}
