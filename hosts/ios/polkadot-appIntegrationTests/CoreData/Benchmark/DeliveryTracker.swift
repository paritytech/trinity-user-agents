import Foundation

/// Records snapshot deliveries per subscription index and lets a test await a delivery count or a
/// snapshot size.
final class DeliveryTracker: @unchecked Sendable {
    private struct Waiter {
        let subscription: Int
        let isSatisfied: (_ deliveries: Int, _ size: Int) -> Bool
        let continuation: CheckedContinuation<Void, Never>
    }

    private let lock = NSLock()
    private var deliveryCounts: [Int: Int] = [:]
    private var lastSizes: [Int: Int] = [:]
    private var lastIdentifiers: [Int: [String]] = [:]
    private var waiters: [Waiter] = []

    func recordDelivery(subscription: Int, size: Int, identifiers: [String] = []) {
        lock.lock()
        let deliveries = deliveryCounts[subscription, default: 0] + 1
        deliveryCounts[subscription] = deliveries
        lastSizes[subscription] = size
        lastIdentifiers[subscription] = identifiers

        let ready = waiters.filter { $0.subscription == subscription && $0.isSatisfied(deliveries, size) }
        waiters.removeAll { $0.subscription == subscription && $0.isSatisfied(deliveries, size) }
        lock.unlock()

        ready.forEach { $0.continuation.resume() }
    }

    func waitForSize(subscription: Int, atLeast minimumSize: Int) async {
        await wait(subscription: subscription) { _, size in size >= minimumSize }
    }

    func waitForDeliveries(subscription: Int, atLeast minimumDeliveries: Int) async {
        await wait(subscription: subscription) { deliveries, _ in deliveries >= minimumDeliveries }
    }

    func deliveries(subscription: Int) -> Int {
        lock.lock()
        defer { lock.unlock() }
        return deliveryCounts[subscription, default: 0]
    }

    func totalDeliveries() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return deliveryCounts.values.reduce(0, +)
    }

    func identifiers(subscription: Int) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return lastIdentifiers[subscription, default: []]
    }
}

private extension DeliveryTracker {
    func wait(subscription: Int, until isSatisfied: @escaping (Int, Int) -> Bool) async {
        await withCheckedContinuation { continuation in
            lock.lock()

            let deliveries = deliveryCounts[subscription, default: 0]
            let size = lastSizes[subscription, default: 0]

            if isSatisfied(deliveries, size) {
                lock.unlock()
                continuation.resume()
                return
            }

            waiters.append(Waiter(subscription: subscription, isSatisfied: isSatisfied, continuation: continuation))
            lock.unlock()
        }
    }
}
