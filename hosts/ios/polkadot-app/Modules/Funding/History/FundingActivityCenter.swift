import Foundation
import Observation
import os
import TrUAPIHost

/// The CASH card's funding lists: sessions still in the core, then the ended
/// ones the host keeps.
///
/// A session that is no longer in flight is written to the host's store, then
/// acknowledged, after which the core drops it. A withdrawal stays in flight
/// until its payout is reported. When a late report brings an acknowledged
/// session back, such as a failed payout's refund, its row is written again.
@MainActor
@Observable
final class FundingActivityCenter {
    static let shared = FundingActivityCenter(store: CoreDataFundingHistoryStore())

    private(set) var inFlight: [FundingActivityItem] = []
    private(set) var history: [FundingActivityItem] = []
    /// Sessions that left In progress while the lists were showing, with when
    /// they did, so their rows arrive in the history with their completion.
    private(set) var justFinished: [String: Date] = [:]

    private var records: [String: FundingRecord] = [:]
    private let store: FundingHistoryStoring
    private var refreshTask: Task<Void, Never>?
    /// Reads the sessions again when one turns to taking longer.
    private var longerTask: Task<Void, Never>?
    private var needsRefresh = false

    init(store: FundingHistoryStoring) {
        self.store = store
    }

    /// The runtime is built off the main thread, so it and its overlay are
    /// handed over through a lock rather than through the main actor.
    nonisolated static func attach(runtime: FundingRuntime, overlay: FundingOverlayPresenting) {
        attached.withLock { attached in
            attached.runtime = runtime
            attached.overlay = overlay
        }
        Task { @MainActor in shared.refresh() }
    }

    /// Shows a session in flight again, such as the bank details to pay to.
    func open(intent: String) {
        Self.attached.withLock { $0.overlay }?.reopen(intent: intent)
    }

    /// Shows an ended session's detail screen.
    func showEnded(intent: String) {
        guard let record = records[intent] else { return }
        Self.attached.withLock { $0.overlay }?.showEnded(record)
    }

    /// Reads the core's sessions again and settles the ended ones. Calls that
    /// arrive while one is running are folded into one more pass.
    func refresh() {
        guard refreshTask == nil else {
            needsRefresh = true
            return
        }

        refreshTask = Task {
            repeat {
                needsRefresh = false
                await reload()
            } while needsRefresh
            refreshTask = nil
        }
    }
}

private extension FundingActivityCenter {
    struct Attached {
        weak var runtime: FundingRuntime?
        weak var overlay: FundingOverlayPresenting?
    }

    nonisolated static let attached = OSAllocatedUnfairLock(initialState: Attached())

    var runtime: FundingRuntime? {
        Self.attached.withLock { $0.runtime }
    }

    func reload() async {
        guard let runtime else {
            await loadStored(coreSessions: [])
            return
        }

        let sessions = runtime.fundingSessions().map { session in
            (session: session, progress: runtime.fundingProgress(intent: session.intent))
        }

        let moving = sessions.filter { Self.isInFlight($0.session, progress: $0.progress) }
        justFinished = Self.finished(
            before: inFlight.map(\.id),
            after: moving.map(\.session.intent),
            known: justFinished,
            now: Date()
        )
        inFlight = moving
            .map { FundingActivityItem(session: $0.session, progress: $0.progress) }
            .sorted { $0.date > $1.date }
        scheduleTakingLonger(moving.compactMap(\.progress?.longerAtMs))

        let ended = sessions
            .filter { !Self.isInFlight($0.session, progress: $0.progress) }
            .compactMap { FundingRecord(session: $0.session, progress: $0.progress) }
        await settle(ended, runtime: runtime)
        await loadStored(coreSessions: ended)
    }

    static func isInFlight(_ session: FundingSession, progress: FundingProgress?) -> Bool {
        progress?.status.isInFlight ?? session.stage.isOpen
    }

    /// Writes each ended session to history, then acknowledges it. A session
    /// whose write or acknowledgement fails stays unacknowledged, so the core
    /// hands it over again.
    func settle(_ ended: [FundingRecord], runtime: FundingRuntime) async {
        for record in ended {
            do {
                try await store.save(record)
                _ = try await runtime.acknowledgeFundingSession(intent: record.intent)
            } catch {
                Logger.shared.error("[funding] \(record.intent): keeping the ended session failed: \(error)")
            }
        }
    }

    /// The core turns a session to taking longer at `longerAtMs` without a
    /// report to announce it, so read the sessions again then.
    func scheduleTakingLonger(_ deadlines: [UInt64]) {
        longerTask?.cancel()
        let now = Date()
        guard let next = deadlines.map(Date.init(milliseconds:)).filter({ $0 > now }).min() else { return }

        longerTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(next.timeIntervalSince(now)))
            guard !Task.isCancelled else { return }
            self?.refresh()
        }
    }

    /// The store's rows, plus any ended session the store could not take, so
    /// nothing the core still holds goes missing from the list.
    func loadStored(coreSessions: [FundingRecord]) async {
        let stored: [FundingRecord]
        do {
            stored = try await store.records()
        } catch {
            Logger.shared.error("[funding] reading the history failed: \(error)")
            stored = []
        }
        let storedIds = Set(stored.map(\.intent))
        let records = stored + coreSessions.filter { !storedIds.contains($0.intent) }

        self.records = Dictionary(records.map { ($0.intent, $0) }) { _, latest in latest }
        let inFlightIds = Set(inFlight.map(\.id))
        history = records
            .filter { !inFlightIds.contains($0.intent) }
            .sorted { $0.settledAt > $1.settledAt }
            .map(FundingActivityItem.init(record:))
    }
}

extension FundingActivityCenter {
    /// The sessions that just left In progress, added to those still within
    /// their arrival window.
    nonisolated static func finished(
        before: [String],
        after: [String],
        known: [String: Date],
        now: Date
    ) -> [String: Date] {
        var result = known.filter { now.timeIntervalSince($0.value) < PurseMotion.Arrival.window }
        for id in Set(before).subtracting(after) {
            result[id] = now
        }
        return result
    }

    /// Whether the row for `id` arrives with its completion now.
    func isArriving(_ id: String, now: Date = Date()) -> Bool {
        justFinished[id].map { now.timeIntervalSince($0) < PurseMotion.Arrival.window } ?? false
    }
}
