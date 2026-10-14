import Foundation

final class DeviceSyncTestEventRecorder<Value: Sendable>: @unchecked Sendable {
    private typealias Waiter = (count: Int, continuation: CheckedContinuation<[Value], Never>)

    private let lock = NSLock()
    private var values = [Value]()
    private var waiters = [Waiter]()

    var snapshot: [Value] { lock.withLock { values } }

    func record(_ value: Value) {
        let resumptions: [(CheckedContinuation<[Value], Never>, [Value])] = lock.withLock {
            values.append(value)
            let ready = waiters.filter { values.count >= $0.count }
            waiters.removeAll { values.count >= $0.count }
            return ready.map { ($0.continuation, Array(values.prefix($0.count))) }
        }
        resumptions.forEach { $0.0.resume(returning: $0.1) }
    }

    func waitForCount(_ count: Int) async -> [Value] {
        await withCheckedContinuation { continuation in
            let immediate: [Value]? = lock.withLock {
                guard values.count < count else { return Array(values.prefix(count)) }
                waiters.append((count, continuation))
                return nil
            }
            if let immediate {
                continuation.resume(returning: immediate)
            }
        }
    }
}
