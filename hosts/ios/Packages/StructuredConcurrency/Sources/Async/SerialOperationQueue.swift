import Foundation

/// Processes async operations strictly one at a time, FIFO.
///
/// Unlike a plain actor — whose `async` methods can interleave at every
/// suspension point — this queue guarantees that an enqueued operation
/// runs to completion before the next one starts, even across `await`s
/// inside the operation. Use it where Kotlin would use a single coroutine
/// consuming a `Channel`.
///
/// Implementation: each `run` chains its operation onto a tail `Task`
/// that awaits the previous tail. Reading and updating `tail` happens
/// synchronously on the actor's executor, so ordering matches the
/// enqueue order observed by the actor.
///
/// Two overloads are provided so callers don't need `try`/`try?` for
/// non-throwing operations. Swift overload resolution prefers the
/// non-throwing variant when the closure can't throw.
///
/// ```swift
/// let queue = SerialOperationQueue()
/// await queue.run { /* non-throwing op */ }
/// try await queue.run { /* throwing op */ }
/// ```
///
/// The queue has no shutdown — it keeps accepting work for as long as
/// it is referenced. Callers gate behaviour via their own lifecycle
/// state (typically checked at the start of each operation) and let
/// the queue be released by ARC when its owner goes away.
public actor SerialOperationQueue {
    private var tail: Task<Void, Never> = Task {}

    public init() {}

    /// Enqueues `operation` behind any previously enqueued operations and
    /// awaits its result. Operations execute in the order `run` was called
    /// on the actor.
    public func run<T: Sendable>(
        _ operation: @Sendable @escaping () async -> T
    ) async -> T {
        let previousTail = tail
        let task = Task<T, Never> {
            await previousTail.value
            return await operation()
        }
        tail = Task { _ = await task.value }
        return await task.value
    }

    /// Throwing overload of ``run(_:)-...``. Rethrows whatever
    /// `operation` throws.
    public func run<T: Sendable>(
        _ operation: @Sendable @escaping () async throws -> T
    ) async throws -> T {
        let previousTail = tail
        let task = Task<T, Error> {
            await previousTail.value
            return try await operation()
        }
        tail = Task { _ = await task.result }
        return try await task.value
    }
}
