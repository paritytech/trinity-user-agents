import Foundation

final class SteppingClock: @unchecked Sendable {
    private let lock = NSLock()
    private let step: Int
    private var next: Int

    init(start: Int, step: Int) {
        next = start
        self.step = step
    }

    func read() -> Int {
        lock.withLock {
            defer { next += step }
            return next
        }
    }
}
