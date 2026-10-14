import AsyncExtensions
import Foundation

@MainActor
public final class StallBoard {
    private let sources: [any StallActivitySource]
    private let revealAfter: TimeInterval
    private let currentDate: @Sendable () -> Date
    private let subject: AsyncCurrentValueSubject<StallReportSnapshot?>
    private var latched: Set<UUID> = []
    private var autoDismissed: Set<UUID> = []
    private var sourceActivities: [[StallActivity]] = []
    private var tickTask: Task<Void, Never>?

    public init(
        sources: [any StallActivitySource],
        revealAfter: TimeInterval = 5,
        currentDate: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.sources = sources
        self.revealAfter = revealAfter
        self.currentDate = currentDate
        subject = AsyncCurrentValueSubject(nil)
        sourceActivities = Array(repeating: [], count: sources.count)

        subscribeToSources()
    }

    public var snapshots: AnyAsyncSequence<StallReportSnapshot?> {
        subject.eraseToAnyAsyncSequence()
    }

    var currentSnapshot: StallReportSnapshot? {
        subject.value
    }

    var ingestedActivities: [StallActivity] {
        sourceActivities.flatMap { $0 }
    }

    var isTicking: Bool {
        tickTask != nil
    }

    public func dismiss(id: UUID) async {
        latched.remove(id)
        for source in sources {
            await source.dismiss(id: id)
        }
        refresh()
    }

    func refresh() {
        let now = currentDate()
        let activities = ingestedActivities

        latched.formIntersection(activities.map(\.id))
        autoDismissed.formIntersection(activities.map(\.id))
        latchRevealed(among: activities, at: now)
        reclaimUnseen(among: activities)
        publishSnapshot(of: activities, at: now)
        updateTick(for: activities)
    }
}

private extension StallBoard {
    func subscribeToSources() {
        for (index, source) in sources.enumerated() {
            Task {
                do {
                    for try await activities in source.activities {
                        await self.updateSourceActivities(index: index, activities: activities)
                    }
                } catch {
                    // Sources never fail; the throwing form comes from `AnyAsyncSequence`.
                }
            }
        }
    }

    func updateSourceActivities(index: Int, activities: [StallActivity]) async {
        sourceActivities[index] = activities
        refresh()
    }

    func latchRevealed(among activities: [StallActivity], at now: Date) {
        for activity in activities where shouldReveal(activity, at: now) {
            latched.insert(activity.id)
        }
    }

    func shouldReveal(_ activity: StallActivity, at now: Date) -> Bool {
        switch activity.visibility {
        case .immediate:
            true
        case .never:
            false
        case .whenStale:
            now.timeIntervalSince(activity.startedAt) >= revealAfter
        }
    }

    /// An activity that ended without ever being revealed was never shown to the user, so the board
    /// reclaims it instead of leaving it for a dismiss tap that will never come.
    func reclaimUnseen(among activities: [StallActivity]) {
        let unseen = activities.filter { activity in
            activity.endedAt != nil && !latched.contains(activity.id) && !autoDismissed.contains(activity.id)
        }

        for activity in unseen {
            autoDismissed.insert(activity.id)
            Task { await self.dismiss(id: activity.id) }
        }
    }

    func publishSnapshot(of activities: [StallActivity], at now: Date) {
        let revealed = activities
            .filter { latched.contains($0.id) }
            .sorted { $0.startedAt < $1.startedAt }

        let snapshot: StallReportSnapshot? = revealed.isEmpty ? nil : StallReportSnapshot(
            activities: revealed.map { makeSnapshotActivity($0, at: now) }
        )

        if subject.value != snapshot {
            subject.value = snapshot
        }
    }

    func makeSnapshotActivity(_ activity: StallActivity, at now: Date) -> StallReportSnapshot.Activity {
        let reference = activity.endedAt ?? now
        return StallReportSnapshot.Activity(
            id: activity.id,
            title: activity.title,
            subtitle: activity.subtitle,
            elapsed: reference.timeIntervalSince(activity.startedAt),
            steps: activity.steps.map { step in
                StallReportSnapshot.Step(
                    id: step.id,
                    title: step.title,
                    depth: step.depth,
                    state: step.state,
                    elapsed: step.finishedAt.map { $0.timeIntervalSince(step.startedAt) }
                        ?? reference.timeIntervalSince(step.startedAt)
                )
            }
        )
    }

    /// Tick while anything is latched with no end time (to refresh elapsed) OR while a `.whenStale`
    /// activity with no end time is still waiting to cross the threshold. Only checking `latched`
    /// deadlocks the reveal: with a non-zero `revealAfter` nothing is latched at first, so no tick
    /// starts, so nothing ever re-evaluates the threshold unless a source happens to publish again.
    /// Ended activities stop the tick — their elapsed is frozen and requires no refresh.
    func updateTick(for activities: [StallActivity]) {
        let hasRunningLatched = activities.contains { activity in
            latched.contains(activity.id) && activity.endedAt == nil
        }

        let hasPendingRunning = activities.contains { activity in
            activity.visibility == .whenStale && !latched.contains(activity.id) && activity.endedAt == nil
        }

        if !hasRunningLatched, !hasPendingRunning {
            tickTask?.cancel()
            tickTask = nil
        } else if tickTask == nil {
            startTick()
        }
    }

    func startTick() {
        tickTask = Task {
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: .seconds(1))
                    refresh()
                } catch {
                    break
                }
            }
        }
    }
}
