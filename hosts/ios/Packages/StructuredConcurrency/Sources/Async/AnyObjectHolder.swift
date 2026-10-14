import Foundation

public final class AnyObjectHolder<T>: @unchecked Sendable {
    private var value: T?
    private let mutex = NSLock()

    public init() {}

    public func get() -> T? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return value
    }

    public func set(_ newValue: T?) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        value = newValue
    }
}
