import Foundation
import FoundationExt
import Observation
import TrUAPIHost

/// Hears which session's flow the router has on screen.
@MainActor
protocol FundingFlowTracking: AnyObject {
    func flowOpened(entryId: String)
    func flowClosed(entryId: String)
}

/// What the screens around the funding overlay ask of the session's funding
/// lists: the tab bar's funding screens attach their router, the CASH card
/// shows the sessions in flight before starting another, and the tab bar
/// asks whether a session's flow is on screen.
@MainActor
protocol FundingActivityCenterProtocol: FundingFlowTracking, Sendable {
    /// Starts following the sessions, shown through `router`.
    func attach(router: FundingFlowRouting)
    /// Shows the sessions in flight in `direction` before starting another.
    /// Returns false, showing nothing, when none is.
    func showPending(direction: FundingDirection) -> Bool
    /// Whether session `entryId`'s flow is on screen.
    func isShowingFlow(entryId: String) -> Bool
}

/// The CASH card's funding lists: the sessions in flight and the ended ones,
/// as the funding service lists them, and the sessions whose flow is on
/// screen.
@MainActor
@Observable
final class FundingActivityCenter: FundingActivityCenterProtocol {
    private(set) var inFlight: [FundingActivityItem] = []
    private(set) var history: [FundingActivityItem] = []
    /// Sessions that left In progress while the lists were showing, with when
    /// they did, so their rows arrive in the history with their completion.
    private(set) var justFinished: [String: Date] = [:]

    @ObservationIgnored private let service: FundingServiceProtocol
    @ObservationIgnored private let clock: any Clock<Duration>
    @ObservationIgnored private let logger: LoggerProtocol
    @ObservationIgnored private weak var router: FundingFlowRouting?
    @ObservationIgnored private var shownFlows: Set<String> = []
    @ObservationIgnored private var followingTask: Task<Void, Never>?
    /// Draws the sessions again when one turns to taking longer.
    @ObservationIgnored private var longerTask: Task<Void, Never>?

    init(
        service: FundingServiceProtocol,
        clock: any Clock<Duration> = ContinuousClock(),
        logger: LoggerProtocol
    ) {
        self.service = service
        self.clock = clock
        self.logger = logger
    }

    func attach(router: FundingFlowRouting) {
        self.router = router
        follow()
    }

    /// Shows a row's session: in flight on its progress screen, ended on its
    /// detail screen.
    func openDetail(_ item: FundingActivityItem) {
        router?.reopen(entryId: item.id)
    }

    func showPending(direction: FundingDirection) -> Bool {
        guard inFlight.contains(where: { $0.direction == direction }), let router else { return false }

        router.showPending(direction: direction)
        return true
    }

    func isShowingFlow(entryId: String) -> Bool {
        shownFlows.contains(entryId)
    }

    func flowOpened(entryId: String) {
        shownFlows.insert(entryId)
    }

    func flowClosed(entryId: String) {
        shownFlows.remove(entryId)
    }

    /// Shows every session, in flight and ended.
    func showHistory() {
        router?.showHistory()
    }

    /// Reads the lists again, as when a session turns to taking longer.
    func refresh() {
        follow()
    }
}

private extension FundingActivityCenter {
    func follow() {
        followingTask?.cancel()
        followingTask = Task { [weak self, service, logger] in
            do {
                for try await entries in service.subscribeEntries() {
                    self?.show(entries)
                }
            } catch {
                if !Task.isCancelled {
                    logger.error("[funding] following the sessions failed: \(error)")
                }
            }
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
        longerTask?.cancel()
        let now = Date()
        guard let next = deadlines.map(Date.init(milliseconds:)).filter({ $0 > now }).min() else { return }

        longerTask = Task { [weak self, clock] in
            guard await (try? clock.sleep(for: .seconds(next.timeIntervalSince(now)))) != nil else { return }
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
