import Foundation
import UserNotifications
@testable import polkadot_app

/// Keeps pending and delivered notifications in memory, the way the system center would report them.
final class FakeScheduledNotificationCenter: ScheduledNotificationCentering, @unchecked Sendable {
    struct AddRefused: Error, Equatable {}

    private let lock = NSLock()
    private var pending: [UNNotificationRequest] = []
    private var delivered: [UNNotification] = []
    private var attempted: [UNNotificationRequest] = []
    private var refusesAdd = false

    /// Every request handed to `add`, including the ones it refused.
    var attemptedRequests: [UNNotificationRequest] {
        lock.withLock { attempted }
    }

    func refuseAdd() {
        lock.withLock { refusesAdd = true }
    }

    /// The pending notifications fire and their banners stay in Notification Center.
    func firePending(at date: Date) {
        lock.withLock {
            delivered += pending.map { FakeScheduledNotificationCenter.notification(request: $0, date: date) }
            pending = []
        }
    }

    /// The pending notifications fire and the user dismisses them, or the system drops them.
    func losePending() {
        lock.withLock { pending = [] }
    }

    func add(_ request: UNNotificationRequest) async throws {
        try lock.withLock {
            attempted.append(request)
            guard !refusesAdd else {
                throw AddRefused()
            }
            pending.append(request)
        }
    }

    func pendingNotificationRequests() async -> [UNNotificationRequest] {
        lock.withLock { pending }
    }

    func deliveredNotifications() async -> [UNNotification] {
        lock.withLock { delivered }
    }

    func removePendingNotificationRequests(withIdentifiers identifiers: [String]) {
        lock.withLock {
            pending.removeAll { identifiers.contains($0.identifier) }
        }
    }
}

private extension FakeScheduledNotificationCenter {
    static func notification(request: UNNotificationRequest, date: Date) -> UNNotification {
        UNNotification(coder: NotificationFieldsCoder(values: ["request": request, "date": date]))!
    }
}

/// `UNNotification` has no public initialiser, so this coder feeds `init(coder:)` the fields it reads.
private final class NotificationFieldsCoder: NSCoder {
    private let values: [String: Any]

    init(values: [String: Any]) {
        self.values = values
    }

    override var allowsKeyedCoding: Bool { true }
    override var requiresSecureCoding: Bool { true }

    override func containsValue(forKey key: String) -> Bool {
        values[key] != nil
    }

    override func decodeObject(forKey key: String) -> Any? {
        values[key]
    }

    override func __decodeObject(ofClasses _: Set<AnyHashable>?, forKey key: String) -> Any? {
        values[key]
    }
}
