import AsyncExtensions
import Foundation
import Testing

@testable import StructuredConcurrency

@Suite("StallBoard")
struct StallBoardTests {
    @Test("nothing published before threshold")
    func nothingPublishedBeforeThreshold() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activity = StallActivity(
            id: UUID(),
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        await MainActor.run { board.refresh() }
        let snap1 = await MainActor.run { board.currentSnapshot }
        #expect(snap1 == nil)

        fakeTime.value = 4.9
        await MainActor.run { board.refresh() }
        let snap2 = await MainActor.run { board.currentSnapshot }
        #expect(snap2 == nil)
    }

    @Test("activity appears after threshold")
    func activityAppearsAfterThreshold() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        await MainActor.run { board.refresh() }
        fakeTime.value = 5.0
        await MainActor.run { board.refresh() }

        let snapshot = await MainActor.run { board.currentSnapshot }
        #expect(snapshot != nil)
        #expect(snapshot?.activities.contains(where: { $0.id == activityId }) == true)
    }

    @Test("immediate visibility reveals with zero elapsed")
    func immediateVisibilityReveals() async throws {
        let date: @Sendable () -> Date = { Date() }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "immediate",
            subtitle: nil,
            startedAt: date(),
            visibility: .immediate,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        await MainActor.run { board.refresh() }

        let snapshot = await MainActor.run { board.currentSnapshot }
        #expect(snapshot != nil)
        #expect(snapshot?.activities.contains(where: { $0.id == activityId }) == true)
    }

    @Test("never visibility never reveals")
    func neverVisibilityNeverReveals() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "never",
            subtitle: nil,
            startedAt: date(),
            visibility: .never,
            steps: []
        )
        await source.push([activity])

        await MainActor.run { board.refresh() }
        fakeTime.value = 100
        await MainActor.run { board.refresh() }

        let snapshot = await MainActor.run { board.currentSnapshot }
        #expect(snapshot?.activities.contains(where: { $0.id == activityId }) != true)
    }

    @Test("disappearing activity is removed")
    func disappearingActivityRemoved() async throws {
        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 0, currentDate: { Date() }) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: Date(),
            visibility: .immediate,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        let snap1 = await MainActor.run { board.currentSnapshot }
        #expect(snap1?.activities.contains(where: { $0.id == activityId }) == true)

        await source.push([])
        try await awaitBoard(board) { $0.ingestedActivities.isEmpty }
        await MainActor.run { board.refresh() }

        let snap2 = await MainActor.run { board.currentSnapshot }
        #expect(snap2?.activities.contains(where: { $0.id == activityId }) != true)
    }

    @Test("dismiss forwards to source")
    func dismissForwardsToSource() async throws {
        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 0, currentDate: { Date() }) }

        let activityId = UUID()
        await board.dismiss(id: activityId)

        let dismissed = source.dismissedIds
        #expect(dismissed.contains(activityId))
    }

    @Test("two sources merge")
    func twoSourcesMerge() async throws {
        let source1 = TestActivitySource()
        let source2 = TestActivitySource()
        let board = await MainActor.run {
            StallBoard(
                sources: [source1, source2],
                revealAfter: 0,
                currentDate: { Date() }
            )
        }

        let id1 = UUID()
        let id2 = UUID()
        let activity1 = StallActivity(
            id: id1,
            title: "source1",
            subtitle: nil,
            startedAt: Date(),
            visibility: .immediate,
            steps: []
        )
        let activity2 = StallActivity(
            id: id2,
            title: "source2",
            subtitle: nil,
            startedAt: Date(timeIntervalSinceNow: 1),
            visibility: .immediate,
            steps: []
        )

        await source1.push([activity1])
        await source2.push([activity2])
        try await awaitBoard(board) { $0.ingestedActivities.count == 2 }
        await MainActor.run { board.refresh() }

        let snapshot = await MainActor.run { board.currentSnapshot }
        #expect(snapshot?.activities.count == 2)
        #expect(snapshot?.activities[0].id == id1)
        #expect(snapshot?.activities[1].id == id2)
    }

    @Test("elapsed recomputes on refresh")
    func elapsedRecomputesOnRefresh() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 0, currentDate: date) }

        let activity = StallActivity(
            id: UUID(),
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .immediate,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        let snap1 = await MainActor.run { board.currentSnapshot }
        let elapsed1 = snap1?.activities.first?.elapsed ?? 0

        fakeTime.value = 2.5
        await MainActor.run { board.refresh() }

        let snap2 = await MainActor.run { board.currentSnapshot }
        let elapsed2 = snap2?.activities.first?.elapsed ?? 0

        #expect(elapsed2 > elapsed1)
    }

    @Test("sequential phases under one root do not restart budget")
    func sequentialPhasesUnderOneRootDoNotRestartBudget() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let report = StalenessReport(currentDate: date, maxSteps: 10)
        let board = await MainActor.run { StallBoard(sources: [report], revealAfter: 5, currentDate: date) }

        try await report.trackActivity("root") {
            // The board ingests through a subscription task, so the root has to have landed before
            // any refresh can reveal it.
            try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

            await markStallRegion("phase1") {
                fakeTime.value = 4.0
            }

            let snap1 = await MainActor.run { board.currentSnapshot }
            #expect(snap1 == nil)

            await markStallRegion("phase2") {
                fakeTime.value = 5.0
            }

            await MainActor.run { board.refresh() }
            let snap2 = await MainActor.run { board.currentSnapshot }
            #expect(snap2 != nil)
            #expect(snap2?.activities.count == 1)
        }
    }

    @Test("unNested sequential activities never reveal")
    func unNestedSequentialActivitiesNeverReveal() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let id1 = UUID()
        let id2 = UUID()
        let id3 = UUID()

        let a1 = StallActivity(
            id: id1,
            title: "a1",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([a1])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }
        var snap = await MainActor.run { board.currentSnapshot }
        #expect(snap == nil)

        fakeTime.value = 4.0
        await source.push([])
        try await awaitBoard(board) { $0.ingestedActivities.isEmpty }

        let a2 = StallActivity(
            id: id2,
            title: "a2",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([a2])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }
        snap = await MainActor.run { board.currentSnapshot }
        #expect(snap == nil)

        fakeTime.value = 8.0
        await source.push([])
        try await awaitBoard(board) { $0.ingestedActivities.isEmpty }

        let a3 = StallActivity(
            id: id3,
            title: "a3",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([a3])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }
        snap = await MainActor.run { board.currentSnapshot }
        #expect(snap == nil)

        fakeTime.value = 12.0
        await MainActor.run { board.refresh() }
        snap = await MainActor.run { board.currentSnapshot }
        #expect(snap == nil)
    }

    @Test("latched activity stays revealed")
    func latchedActivityStaysRevealed() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        fakeTime.value = 5.0
        await MainActor.run { board.refresh() }

        let snap1 = await MainActor.run { board.currentSnapshot }
        #expect(snap1?.activities.contains(where: { $0.id == activityId }) == true)

        fakeTime.value = 5.1
        await MainActor.run { board.refresh() }

        let snap2 = await MainActor.run { board.currentSnapshot }
        #expect(snap2?.activities.contains(where: { $0.id == activityId }) == true)
    }

    @Test("snapshots stream publishes on reveal")
    func snapshotsStreamPublishesOnReveal() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )

        // Subscribe to snapshots BEFORE the state change; stream will replay current nil
        let snapshotTask = Task<StallReportSnapshot?, Error> {
            let snapshots = await MainActor.run { board.snapshots }
            var foundNonNil: StallReportSnapshot?
            for try await snapshot in snapshots {
                if snapshot != nil {
                    foundNonNil = snapshot
                    break
                }
            }
            if let found = foundNonNil {
                return found
            }
            throw SnapshotsStreamTimeout()
        }

        // Now trigger the state change
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        fakeTime.value = 5.0
        await MainActor.run { board.refresh() }

        // Wait for the snapshot with timeout guard
        let result = try await withThrowingTaskGroup(of: StallReportSnapshot?.self) { group in
            group.addTask {
                try await snapshotTask.value
            }
            group.addTask {
                try await Task.sleep(for: .seconds(2))
                throw SnapshotsStreamTimeout()
            }
            let snapshot = try await group.next()!
            group.cancelAll()
            return snapshot
        }

        #expect(result != nil)
        #expect(result?.activities.contains(where: { $0.id == activityId }) == true)
    }

    @Test("a pending whenStale activity starts the tick")
    func pendingWhenStaleActivityStartsTick() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activity = StallActivity(
            id: UUID(),
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        let snapshot = await MainActor.run { board.currentSnapshot }
        let isTicking = await MainActor.run { board.isTicking }

        #expect(snapshot == nil)
        #expect(isTicking == true)
    }

    @Test("a never-visibility activity alone does not start the tick")
    func neverVisibilityAloneDoesNotStartTick() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activity = StallActivity(
            id: UUID(),
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .never,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        let isTicking = await MainActor.run { board.isTicking }

        #expect(isTicking == false)
    }

    @Test("the tick stops once nothing is latched or pending")
    func tickStopsWhenNothingLatchedOrPending() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activity = StallActivity(
            id: UUID(),
            title: "test",
            subtitle: nil,
            startedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        let isTicking1 = await MainActor.run { board.isTicking }
        #expect(isTicking1 == true)

        await source.push([])
        try await awaitBoard(board) { $0.ingestedActivities.isEmpty }
        await MainActor.run { board.refresh() }

        let isTicking2 = await MainActor.run { board.isTicking }
        #expect(isTicking2 == false)
    }

    @Test("latched ended activity stays in snapshot")
    func latchedEndedActivityStaysInSnapshot() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 0, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            endedAt: nil,
            visibility: .immediate,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        let snap1 = await MainActor.run { board.currentSnapshot }
        #expect(snap1?.activities.contains(where: { $0.id == activityId }) == true)

        fakeTime.value = 1.0
        let endedActivity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: activity.startedAt,
            endedAt: date(),
            visibility: .immediate,
            steps: []
        )
        await source.push([endedActivity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 && $0.ingestedActivities.first?.endedAt != nil }
        await MainActor.run { board.refresh() }

        let snap2 = await MainActor.run { board.currentSnapshot }
        #expect(snap2?.activities.contains(where: { $0.id == activityId }) == true)
    }

    @Test("ended activity elapsed is frozen")
    func endedActivityElapsedFrozen() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 0, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            endedAt: nil,
            visibility: .immediate,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        fakeTime.value = 2.0
        let endedActivity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: activity.startedAt,
            endedAt: date(),
            visibility: .immediate,
            steps: []
        )
        await source.push([endedActivity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 && $0.ingestedActivities.first?.endedAt != nil }
        await MainActor.run { board.refresh() }

        let snap1 = await MainActor.run { board.currentSnapshot }
        let elapsed1 = snap1?.activities.first?.elapsed ?? 0

        fakeTime.value = 5.0
        await MainActor.run { board.refresh() }

        let snap2 = await MainActor.run { board.currentSnapshot }
        let elapsed2 = snap2?.activities.first?.elapsed ?? 0

        #expect(elapsed1 == elapsed2)
        #expect(elapsed1 == 2.0)
    }

    @Test("ended activity never latched is auto-dismissed")
    func endedActivityNeverLatchedIsAutoDismissed() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            endedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        let snapshot = await MainActor.run { board.currentSnapshot }
        #expect(snapshot == nil)

        for _ in 0 ..< 1_000 {
            if source.dismissedIds.contains(activityId) {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        let dismissed = source.dismissedIds
        #expect(dismissed.contains(activityId))
    }

    @Test("auto-dismiss fires only once across refresh calls")
    func autoDismissFiresOnceOnly() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            endedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }
        await MainActor.run { board.refresh() }

        for _ in 0 ..< 1_000 {
            if source.dismissedIds.contains(activityId) {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        let dismissCount1 = source.dismissedIds.count
        #expect(dismissCount1 == 1)

        await MainActor.run { board.refresh() }
        try await Task.sleep(for: .milliseconds(50))

        let dismissCount2 = source.dismissedIds.count
        #expect(dismissCount2 == 1)
    }

    @Test("isTicking false when only latched activity is ended")
    func isTickingFalseWhenOnlyLatchedActivityEnded() async throws {
        let fakeTime = Ref<TimeInterval>(0)
        let date: @Sendable () -> Date = {
            Date(timeIntervalSince1970: fakeTime.value)
        }

        let source = TestActivitySource()
        let board = await MainActor.run { StallBoard(sources: [source], revealAfter: 5, currentDate: date) }

        let activityId = UUID()
        let activity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: date(),
            endedAt: nil,
            visibility: .whenStale,
            steps: []
        )
        await source.push([activity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 }

        fakeTime.value = 5.0
        await MainActor.run { board.refresh() }

        let isTicking1 = await MainActor.run { board.isTicking }
        #expect(isTicking1 == true)

        let endedActivity = StallActivity(
            id: activityId,
            title: "test",
            subtitle: nil,
            startedAt: activity.startedAt,
            endedAt: date(),
            visibility: .whenStale,
            steps: []
        )
        await source.push([endedActivity])
        try await awaitBoard(board) { $0.ingestedActivities.count == 1 && $0.ingestedActivities.first?.endedAt != nil }
        await MainActor.run { board.refresh() }

        let isTicking2 = await MainActor.run { board.isTicking }
        #expect(isTicking2 == false)
    }
}

final class TestActivitySource: StallActivitySource {
    private let activitiesSubject: AsyncCurrentValueSubject<[StallActivity]>
    private let dismissedIdsSubject: AsyncCurrentValueSubject<Set<UUID>>

    init() {
        activitiesSubject = AsyncCurrentValueSubject([])
        dismissedIdsSubject = AsyncCurrentValueSubject([])
    }

    var activities: AnyAsyncSequence<[StallActivity]> {
        activitiesSubject.eraseToAnyAsyncSequence()
    }

    func push(_ activities: [StallActivity]) async {
        activitiesSubject.value = activities
    }

    func dismiss(id: UUID) async {
        var dismissed = dismissedIdsSubject.value
        dismissed.insert(id)
        dismissedIdsSubject.value = dismissed
    }

    var dismissedIds: Set<UUID> {
        dismissedIdsSubject.value
    }
}

private class Ref<T> {
    var value: T

    init(_ value: T) {
        self.value = value
    }
}

private func awaitBoard(
    _ board: StallBoard,
    timeout: Duration = .seconds(30),
    condition: @escaping @MainActor (StallBoard) -> Bool
) async throws {
    // Both waits are child tasks, so cancelAll() stops the loser and the call returns as soon as one finishes.
    try await withThrowingTaskGroup(of: Void.self) { group in
        group.addTask {
            while await !MainActor.run(body: { condition(board) }) {
                try await Task.sleep(for: .milliseconds(10))
            }
        }

        group.addTask {
            try await Task.sleep(for: timeout)
            throw AwaitBoardTimeout()
        }

        _ = try await group.next()
        group.cancelAll()
    }
}

private struct AwaitBoardTimeout: Error {}

private struct SnapshotsStreamTimeout: Error {}
