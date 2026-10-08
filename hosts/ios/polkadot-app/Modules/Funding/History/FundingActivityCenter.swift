import Foundation
import Observation
import os
import TrUAPIHost

/// The CASH card's funding lists: sessions still in the core, then the ended
/// ones the host keeps.
///
/// An ended session is written to the host's store, then acknowledged, after
/// which the core drops it. Outbound value waits for its payout outcome, or a
/// day, before it is acknowledged, and its row is rewritten if the payout
/// arrives in that time.
@MainActor
@Observable
final class FundingActivityCenter {
    static let shared = FundingActivityCenter(store: CoreDataFundingHistoryStore())

    /// What the user can spend now, as the CASH card last showed it.
    var spendable: Decimal?

    private(set) var inFlight: [FundingActivityItem] = []
    private(set) var history: [FundingActivityItem] = []

    private let store: FundingHistoryStoring
    private var refreshTask: Task<Void, Never>?
    private var needsRefresh = false

    init(store: FundingHistoryStoring) {
        self.store = store
    }

    /// The runtime is built off the main thread, so it is handed over through
    /// a lock rather than through the main actor.
    nonisolated static func attach(runtime: FundingRuntime) {
        attached.withLock { $0.runtime = runtime }
        Task { @MainActor in shared.refresh() }
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

        inFlight = sessions
            .filter(\.session.stage.isOpen)
            .map { FundingActivityItem(session: $0.session, progress: $0.progress) }

        let ended = sessions.compactMap { entry in
            FundingRecord(session: entry.session, progress: entry.progress)
        }
        await settle(ended, runtime: runtime)
        await loadStored(coreSessions: ended)
    }

    func settle(_ ended: [FundingRecord], runtime: FundingRuntime) async {
        for record in ended {
            do {
                try await store.save(record)
                guard record.isFinal() else { continue }
                _ = try await runtime.acknowledgeFundingSession(intent: record.intent)
            } catch {
                continue
            }
        }
    }

    /// The store's rows, plus any ended session the store could not take, so
    /// nothing the core still holds goes missing from the list.
    func loadStored(coreSessions: [FundingRecord]) async {
        let stored = await (try? store.records()) ?? []
        let storedIds = Set(stored.map(\.intent))
        let records = stored + coreSessions.filter { !storedIds.contains($0.intent) }

        history = records
            .sorted { $0.settledAt > $1.settledAt }
            .map(FundingActivityItem.init(record:))
    }
}
