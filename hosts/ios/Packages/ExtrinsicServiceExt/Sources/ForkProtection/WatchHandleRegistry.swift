import Foundation
import os

final class WatchHandleRegistry: @unchecked Sendable {
    private let mutex = OSAllocatedUnfairLock()
    private var handles: [UInt16: Handle] = [:]
    private var lastHandleId: UInt16 = 0

    func makeHandle() -> Handle? {
        mutex.lock()
        defer { mutex.unlock() }

        guard handles.count < Int(UInt16.max) else { return nil }

        repeat {
            lastHandleId = lastHandleId &+ 1
        } while lastHandleId == 0 || handles[lastHandleId] != nil

        let handle = Handle(id: lastHandleId)
        handles[handle.id] = handle

        return handle
    }

    func beginAttempt(_ handle: Handle) {
        mutex.withLock { handle.isAttemptActive = true }
    }

    func assign(task: Task<Void, Never>, to handle: Handle) {
        mutex.lock()
        let isCancelled = handle.isCancelled
        handle.task = task
        mutex.unlock()

        guard isCancelled else { return }

        task.cancel()
    }

    func updateSubscriptionId(_ id: UInt16, handle: Handle) -> UInt16? {
        mutex.lock()
        let isStale = handle.isCancelled || !handle.isAttemptActive
        handle.currentSubscriptionId = isStale ? nil : id
        mutex.unlock()

        return isStale ? id : nil
    }

    func takeSubscriptionId(_ handle: Handle) -> UInt16? {
        mutex.lock()
        defer { mutex.unlock() }

        let id = handle.currentSubscriptionId
        handle.currentSubscriptionId = nil
        handle.isAttemptActive = false

        return id
    }

    func removeHandle(_ handle: Handle) {
        mutex.lock()
        defer { mutex.unlock() }

        handles[handle.id] = nil
    }

    func cancel(id: UInt16) -> UInt16? {
        mutex.lock()
        let handle = handles[id]
        handles[id] = nil
        handle?.isCancelled = true
        let task = handle?.task
        let currentId = handle?.currentSubscriptionId
        handle?.currentSubscriptionId = nil
        mutex.unlock()

        task?.cancel()

        return currentId
    }
}

extension WatchHandleRegistry {
    final class Handle: @unchecked Sendable {
        let id: UInt16
        var task: Task<Void, Never>?
        var isCancelled = false
        var isAttemptActive = false
        var currentSubscriptionId: UInt16?

        init(id: UInt16) {
            self.id = id
        }
    }
}
