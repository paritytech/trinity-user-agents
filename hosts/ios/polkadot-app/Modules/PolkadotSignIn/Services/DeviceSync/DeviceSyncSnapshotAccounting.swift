struct DeviceSyncSnapshotAccounting {
    enum Acknowledgement {
        case unknown
        case inconsistent
        case recorded(Chat.DeviceSyncUpdate)
        case snapshotCompleted(Chat.DeviceSyncUpdate, timePoint: UInt64)
    }

    private struct Snapshot {
        var remaining: Set<UInt32>
        let timePoint: UInt64
    }

    private var pendingUpdates = [UInt32: Chat.DeviceSyncUpdate]()
    private var snapshot: Snapshot?
    private var hasDeferredTimeout = false

    var hasPendingUpdates: Bool { !pendingUpdates.isEmpty }

    mutating func beginSnapshot(updates: [Chat.DeviceSyncUpdate], timePoint: UInt64) {
        snapshot = Snapshot(
            remaining: Set(updates.map(\.id)),
            timePoint: timePoint
        )
    }

    mutating func registerPending(_ update: Chat.DeviceSyncUpdate) {
        pendingUpdates[update.id] = update
    }

    mutating func rollBackSnapshot(sentIds: [UInt32]) {
        sentIds.forEach { pendingUpdates.removeValue(forKey: $0) }
        snapshot = nil
        hasDeferredTimeout = false
    }

    mutating func acknowledge(updateId: UInt32) -> Acknowledgement {
        guard let update = pendingUpdates[updateId] else {
            return .unknown
        }
        guard var snapshot, snapshot.remaining.contains(updateId) else {
            return .inconsistent
        }

        pendingUpdates.removeValue(forKey: updateId)
        snapshot.remaining.remove(updateId)

        if snapshot.remaining.isEmpty {
            self.snapshot = nil
            return .snapshotCompleted(update, timePoint: snapshot.timePoint)
        }

        self.snapshot = snapshot
        return .recorded(update)
    }

    func containsPending(updateId: UInt32) -> Bool {
        pendingUpdates[updateId] != nil
    }

    mutating func deferTimeout() {
        hasDeferredTimeout = true
    }

    mutating func consumeDeferredTimeout() -> Bool {
        defer { hasDeferredTimeout = false }
        return hasDeferredTimeout
    }

    mutating func reset() {
        pendingUpdates.removeAll()
        snapshot = nil
        hasDeferredTimeout = false
    }
}
