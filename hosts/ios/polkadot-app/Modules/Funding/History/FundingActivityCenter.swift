import Foundation
import Observation
import os
import TrUAPIHost

/// The CASH card's funding lists: the sessions in flight the core last
/// pushed, then the ended ones from the core's history, read again on every
/// refresh.
///
/// An ended session is acknowledged while the history shows it, after which
/// the core stops handing it over on each resume.
@MainActor
@Observable
final class FundingActivityCenter {
    static let shared = FundingActivityCenter()

    private(set) var inFlight: [FundingActivityItem] = []
    private(set) var history: [FundingActivityItem] = []
    /// Sessions that left In progress while the lists were showing, with when
    /// they did, so their rows arrive in the history with their completion.
    private(set) var justFinished: [String: Date] = [:]

    private var historyShown = 0
    private var refreshTask: Task<Void, Never>?
    /// Draws the sessions again when one turns to taking longer.
    private var longerTask: Task<Void, Never>?
    private var needsRefresh = false

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

    /// Shows a row's session: in flight on its progress screen, ended on its
    /// detail screen.
    func openDetail(_ item: FundingActivityItem) {
        open(intent: item.id)
    }

    /// Shows the sessions in flight in `direction` before starting another.
    /// Returns false, showing nothing, when none is.
    func showPending(direction: FundingDirection) -> Bool {
        guard inFlight.contains(where: { $0.direction == direction }),
              let overlay = Self.attached.withLock({ $0.overlay })
        else { return false }
        overlay.showPending(direction: direction)
        return true
    }

    /// Shows every session, in flight and ended.
    func showHistory() {
        Self.attached.withLock { $0.overlay }?.showHistory()
    }

    /// The history is on screen until the matching ``endShowingHistory()``,
    /// so the ended sessions it draws are acknowledged.
    func beginShowingHistory() {
        historyShown += 1
        refresh()
    }

    func endShowingHistory() {
        historyShown -= 1
    }

    /// Draws the sessions the core last pushed and its history. Calls that
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
        let runtime = runtime
        let (entries, summaries) = await Task.detached {
            (runtime?.fundingEntries() ?? [], runtime?.fundingHistory() ?? [])
        }.value

        let moving = entries.filter(\.progress.status.isInFlight)
        justFinished = Self.finished(
            before: inFlight.map(\.id),
            after: moving.map(\.session.intent),
            known: justFinished,
            now: Date()
        )
        inFlight = moving
            .map { FundingActivityItem(session: $0.session, progress: $0.progress) }
            .sorted { $0.date > $1.date }
        scheduleTakingLonger(moving.compactMap(\.progress.longerAtMs))

        let inFlightIds = Set(inFlight.map(\.id))
        history = summaries
            .filter { !inFlightIds.contains($0.intent) }
            .compactMap(FundingActivityItem.init(summary:))

        guard historyShown > 0, let runtime else { return }
        let owed = Set(entries.filter { !$0.session.acknowledged }.map(\.session.intent))
        for item in history where owed.contains(item.id) {
            await runtime.acknowledgeShown(intent: item.id, logger: Logger.shared)
        }
    }

    /// The core turns a session to taking longer at `longerAtMs` without a
    /// report to announce it, so draw the sessions again then.
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
        var result = known.filter { now.timeIntervalSince($0.value) < FundingActivityTiming.Arrival.window }
        for id in Set(before).subtracting(after) {
            result[id] = now
        }
        return result
    }

    /// Whether the row for `id` arrives with its completion now.
    func isArriving(_ id: String, now: Date = Date()) -> Bool {
        justFinished[id].map { now.timeIntervalSince($0) < FundingActivityTiming.Arrival.window } ?? false
    }
}
