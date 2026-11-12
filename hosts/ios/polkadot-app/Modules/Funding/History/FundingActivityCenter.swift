import Foundation
import Observation
import os
import TrUAPIHost

/// The CASH card's funding lists: the sessions in flight and the ended ones,
/// as the funding service lists them.
@MainActor
@Observable
final class FundingActivityCenter {
    static let shared = FundingActivityCenter()

    private(set) var inFlight: [FundingActivityItem] = []
    private(set) var history: [FundingActivityItem] = []
    /// Sessions that left In progress while the lists were showing, with when
    /// they did, so their rows arrive in the history with their completion.
    private(set) var justFinished: [String: Date] = [:]

    @ObservationIgnored private let following = FundingFollowing()
    /// Draws the sessions again when one turns to taking longer.
    @ObservationIgnored private let longer = FundingFollowing()

    /// The service and router are built with the runtime, off the main
    /// thread, so they are handed over through a lock rather than through the
    /// main actor.
    nonisolated static func attach(service: FundingServiceProtocol, router: FundingFlowRouting) {
        attached.withLock { attached in
            attached.service = service
            attached.router = router
        }
        Task { @MainActor in shared.follow() }
    }

    /// Shows a row's session: in flight on its progress screen, ended on its
    /// detail screen.
    func openDetail(_ item: FundingActivityItem) {
        Self.attached.withLock { $0.router }?.reopen(entryId: item.id)
    }

    /// Shows the sessions in flight in `direction` before starting another.
    /// Returns false, showing nothing, when none is.
    func showPending(direction: FundingDirection) -> Bool {
        guard inFlight.contains(where: { $0.direction == direction }),
              let router = Self.attached.withLock({ $0.router })
        else { return false }
        router.showPending(direction: direction)
        return true
    }

    /// Shows every session, in flight and ended.
    func showHistory() {
        Self.attached.withLock { $0.router }?.showHistory()
    }

    /// Reads the lists again, as when a session turns to taking longer.
    func refresh() {
        follow()
    }
}

private extension FundingActivityCenter {
    struct Attached {
        weak var service: FundingServiceProtocol?
        weak var router: FundingFlowRouting?
    }

    nonisolated static let attached = OSAllocatedUnfairLock(initialState: Attached())

    func follow() {
        guard let service = Self.attached.withLock({ $0.service }) else { return }

        following.task?.cancel()
        following.task = Task { [weak self] in
            do {
                for try await entries in service.subscribeEntries() {
                    self?.show(entries)
                }
            } catch {}
        }
    }

    func show(_ entries: [FundingEntry]) {
        let lists = FundingActivityItem.lists(from: entries)
        justFinished = Self.finished(
            before: inFlight.map(\.id),
            after: lists.inFlight.map(\.id),
            known: justFinished,
            now: Date()
        )
        inFlight = lists.inFlight
        history = lists.history
        scheduleTakingLonger(entries.compactMap { $0.progress?.longerAtMs })
    }

    /// The core turns a session to taking longer at `longerAtMs` without a
    /// change to announce it, so read the sessions again then.
    func scheduleTakingLonger(_ deadlines: [UInt64]) {
        longer.task?.cancel()
        let now = Date()
        guard let next = deadlines.map(Date.init(milliseconds:)).filter({ $0 > now }).min() else { return }

        longer.task = Task { [weak self] in
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
