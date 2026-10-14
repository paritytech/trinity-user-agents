import AsyncExtensions
import Foundation
import Testing

@testable import StructuredConcurrency

@Suite("StalenessReport")
struct StalenessReportTests {
    @Test("callee region nests under caller")
    func calleeRegionNestsUnderCaller() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            await markStallRegion("child") {}

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count == 1)
            #expect(activities[0].title == "root")
            #expect(activities[0].steps[0].title == "child")
            #expect(activities[0].steps[0].depth == 0)
        }
    }

    @Test("default collector is NoOp")
    func defaultCollectorIsNoOp() async throws {
        await markStallRegion("orphan") {}
    }

    @Test("end on parent closes children")
    func endOnParentClosesChildren() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            let handle = StalenessDiagnostics.collector as! StalenessRegionHandling
            let child = handle.startRegion(StallableRegion(title: "child"))
            _ = child.startRegion(StallableRegion(title: "grandchild"))
            child.end()

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count == 2)
            #expect(activities[0].steps.allSatisfy { $0.state == .finished })
        }
    }

    @Test("throwing body fails region")
    func throwingBodyFailsRegion() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        await report.trackActivity("root") {
            do {
                try await markStallRegion("child") {
                    throw TestError()
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count == 1)
            // Single failing region with no ancestors keeps the error detail
            if case let .failed(detail: childDetail) = activities[0].steps[0].state {
                #expect(childDetail != nil)
            } else {
                Issue.record("child step should be failed with detail")
            }
        }
    }

    @Test("cancelled body closes region")
    func cancelledBodyClosesRegion() async throws {
        let report = StalenessReport()

        let childStarted = AsyncCurrentValueSubject<Bool>(false)

        let task = Task<Void, Error> {
            try await report.trackActivity("root") {
                try await markStallRegion("child") {
                    childStarted.value = true
                    try await Task.sleep(for: .seconds(10))
                }
            }
        }

        for await _ in childStarted where childStarted.value {
            break
        }

        task.cancel()
        _ = try? await task.value

        await report.drain()
        let activities = report.subject.value
        let activity = try #require(activities.first)
        #expect(activity.endedAt != nil)
        #expect(activities[0].steps.contains { $0.title == "child" })
        #expect(activities[0].steps.allSatisfy { $0.state != .running })

        await report.dismiss(id: activity.id)
        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("end is idempotent")
    func endIsIdempotent() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            let handle = StalenessDiagnostics.collector as! StalenessRegionHandling
            let child = handle.startRegion(StallableRegion(title: "child"))
            child.end()
            child.end()

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count == 1)
            #expect(activities[0].steps[0].state == .finished)
        }
    }

    @Test("finished steps remain with frozen finishedAt")
    func finishedStepsRemainFrozen() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            await markStallRegion("child") {}

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count == 1)
            #expect(activities[0].steps[0].state == .finished)

            let step = activities[0].steps[0]
            #expect(step.state == .finished)
            #expect(step.finishedAt != nil)

            let finishedAtValue = step.finishedAt
            await report.drain()
            let activities2 = report.subject.value
            #expect(activities2[0].steps[0].finishedAt == finishedAtValue)
        }
    }

    @Test("root ended retains the activity until dismissed")
    func rootEndedRetainsActivityUntilDismissed() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {}

        await report.drain()
        let activities = report.subject.value
        #expect(activities.count == 1)
        #expect(activities[0].endedAt != nil)
        #expect(activities[0].steps.allSatisfy { $0.state == .finished })

        await report.dismiss(id: activities[0].id)

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("unstructured Task inherits collector")
    func unstructuredTaskInheritsCollector() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            let finished = AsyncCurrentValueSubject<Bool>(false)
            Task {
                await markStallRegion("inherited") {}
                finished.value = true
            }

            for await _ in finished where finished.value {
                break
            }

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.contains { $0.title == "inherited" })
        }
    }

    @Test("Task outliving end() is dropped")
    func taskOutlivingEndIsDropped() async throws {
        let report = StalenessReport()

        var task: Task<Void, Never>?

        await report.trackActivity("root") {
            task = Task {
                try? await Task.sleep(for: .seconds(1))
                await markStallRegion("orphan") {}
            }
        }

        _ = await task!.value
        await report.drain()
        let activities = report.subject.value
        let activity = try #require(activities.first)
        #expect(activity.endedAt != nil)
        #expect(activities[0].steps.contains { $0.title == "orphan" } == false)

        await report.dismiss(id: activity.id)
        await report.drain()
        #expect(report.subject.value.isEmpty)

        task?.cancel()
    }

    @Test("Task.detached does not inherit collector")
    func taskDetachedDoesNotInherit() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            let finished = AsyncCurrentValueSubject<Bool>(false)
            Task.detached {
                await markStallRegion("detached") {}
                finished.value = true
            }

            for await _ in finished where finished.value {
                break
            }

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.isEmpty)
        }
    }

    @Test("two concurrent trackActivity produce independent roots")
    func concurrentTrackActivityIndependent() async throws {
        let report = StalenessReport()
        let gate = AsyncCurrentValueSubject<Bool>(false)
        let aStarted = AsyncCurrentValueSubject<Bool>(false)
        let bStarted = AsyncCurrentValueSubject<Bool>(false)

        async let a = await report.trackActivity("root1") {
            aStarted.value = true
            for await _ in gate where gate.value {
                break
            }
        }

        async let b = await report.trackActivity("root2") {
            bStarted.value = true
            for await _ in gate where gate.value {
                break
            }
        }

        for await _ in aStarted where aStarted.value {
            break
        }
        for await _ in bStarted where bStarted.value {
            break
        }

        await report.drain()
        let concurrent = report.subject.value
        #expect(concurrent.count == 2)

        gate.value = true
        _ = await (a, b)
        await report.drain()
        let activities = report.subject.value
        #expect(activities.count == 2)
        #expect(activities.allSatisfy { $0.endedAt != nil })

        for activity in activities {
            await report.dismiss(id: activity.id)
        }

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("nested trackActivity nests as region")
    func nestedTrackActivityNests() async throws {
        let report = StalenessReport()

        await report.trackActivity("outer") {
            await report.trackActivity("inner") {
                await report.drain()
                let activities = report.subject.value
                #expect(activities.count == 1)
                #expect(activities[0].steps.count == 1)
                #expect(activities[0].title == "outer")
                #expect(activities[0].steps[0].title == "inner")
            }
        }

        await report.drain()
        let activities = report.subject.value
        #expect(activities.count == 1)
        #expect(activities[0].endedAt != nil)
        #expect(activities[0].steps.allSatisfy { $0.state == .finished })

        await report.dismiss(id: activities[0].id)

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("dismiss suppresses still-running activity")
    func dismissSuppresseActivity() async throws {
        let report = StalenessReport()

        var activityId: UUID?
        var task: Task<Void, Error>?

        let activityStarted = AsyncCurrentValueSubject<Bool>(false)

        task = Task {
            try await report.trackActivity("root") {
                activityId = (StalenessDiagnostics.collector as! StallHandle).id
                activityStarted.value = true
                try await Task.sleep(for: .seconds(10))
            }
        }

        for await _ in activityStarted where activityStarted.value {
            break
        }

        if let id = activityId {
            await report.dismiss(id: id)
        }

        task?.cancel()
        _ = try? await task?.value

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("step cap overflow drops finished first")
    func stepCapOverflow() async throws {
        let report = StalenessReport(maxSteps: 4)

        await report.trackActivity("root") {
            for i in 0 ..< 10 {
                await markStallRegion("step\(i)") {}
            }

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.count <= 4)
            #expect(activities[0].steps.allSatisfy { $0.state == .finished })
        }
    }

    @Test("events with unknown parent are ignored")
    func unknownParentIgnored() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            let unknownId = UUID()
            report.eventContinuation.yield(.regionStarted(
                id: UUID(),
                parent: unknownId,
                title: "orphan",
                at: Date()
            ))

            await report.drain()
            let activities = report.subject.value
            #expect(activities.count == 1)
            #expect(activities[0].steps.isEmpty)
        }
    }

    @Test("dismissed activity restarts fresh")
    func dismissedActivityRestartsFresh() async throws {
        let report = StalenessReport()

        var activityId: UUID?
        var task: Task<Void, Error>?

        let activityStarted = AsyncCurrentValueSubject<Bool>(false)

        task = Task {
            try await report.trackActivity("root") {
                activityId = (StalenessDiagnostics.collector as! StallHandle).id
                activityStarted.value = true
                try await Task.sleep(for: .seconds(10))
            }
        }

        for await _ in activityStarted where activityStarted.value {
            break
        }

        if let id = activityId {
            await report.dismiss(id: id)
        }

        task?.cancel()
        _ = try? await task?.value

        await report.drain()
        #expect(report.subject.value.isEmpty)

        await report.trackActivity("fresh") {
            await report.drain()
            let activities = report.subject.value
            #expect(activities.contains { $0.title == "fresh" })
        }
    }

    @Test("a throwing region flips activity visibility to immediate")
    func throwingRegionFlipsActivityVisibilityToImmediate() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        try await report.trackActivity("root") {
            do {
                try await markStallRegion("child") {
                    throw TestError()
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first?.visibility == .immediate)
        }
    }

    @Test("a failed root activity stays published")
    func failedRootActivityStaysPublished() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        do {
            try await report.trackActivity("root") {
                throw TestError()
            }
        } catch {}

        await report.drain()
        let activities = report.subject.value
        #expect(activities.count == 1)
        #expect(activities[0].title == "root")
    }

    @Test("a failed root activity is removed once dismissed")
    func failedRootActivityIsRemovedOnceDismissed() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        var activityId: UUID?

        do {
            try await report.trackActivity("root") {
                activityId = (StalenessDiagnostics.collector as! StallHandle).id
                throw TestError()
            }
        } catch {}

        await report.drain()
        let beforeDismiss = report.subject.value
        #expect(beforeDismiss.count == 1)

        if let id = activityId {
            await report.dismiss(id: id)
        }

        await report.drain()
        let activities = report.subject.value
        #expect(activities.isEmpty)
    }

    @Test("failure propagates to the enclosing region")
    func failurePropagatesEnclosingRegion() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        try await report.trackActivity("root") {
            do {
                try await markStallRegion("outer") {
                    try await markStallRegion("inner") {
                        throw TestError()
                    }
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first != nil)
            let steps = activities[0].steps
            // Both regions are failed (state propagates), but only the deepest (inner)
            // carries the error detail; outer (ancestor) has nil.
            try #require(steps.count == 2)
            #expect(steps[0].state == .failed(detail: nil))
            if case let .failed(detail: innerDetail) = steps[1].state {
                #expect(innerDetail != nil)
            } else {
                Issue.record("inner step should be failed")
            }
        }
    }

    @Test("a cancelled region is not marked failed")
    func cancelledRegionNotMarkedFailed() async throws {
        let report = StalenessReport()

        let childStarted = AsyncCurrentValueSubject<Bool>(false)

        try await report.trackActivity("root") {
            let task = Task<Void, Error> {
                try await markStallRegion("child") {
                    childStarted.value = true
                    try await Task.sleep(for: .seconds(10))
                }
            }

            for await _ in childStarted where childStarted.value {
                break
            }

            task.cancel()
            _ = try? await task.value

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first?.steps.first != nil)
            #expect(activities[0].steps[0].state == .finished)
            #expect(activities[0].visibility == .whenStale)
        }
    }

    @Test("a sibling region that succeeded stays finished when another fails")
    func siblingRegionSucceededWhenAnotherFails() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        try await report.trackActivity("root") {
            await markStallRegion("first") {}

            do {
                try await markStallRegion("second") {
                    throw TestError()
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first != nil)
            let steps = activities[0].steps
            try #require(steps.count == 2)
            #expect(steps[0].state == .finished)
            // Second sibling has no ancestors, so it keeps the error detail
            if case let .failed(detail: secondDetail) = steps[1].state {
                #expect(secondDetail != nil)
            } else {
                Issue.record("second step should be failed with detail")
            }
        }
    }

    @Test("a successful region is unaffected")
    func successfulRegionUnaffected() async throws {
        let report = StalenessReport()

        try await report.trackActivity("root") {
            await markStallRegion("child") {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first?.steps.first != nil)
            #expect(activities[0].steps[0].state == .finished)
            #expect(activities[0].visibility == .whenStale)
        }
    }

    @Test("only the deepest failure carries the error text")
    func onlyDeepestFailureCarriesErrorText() async throws {
        let report = StalenessReport()

        struct TestError: Error {}

        try await report.trackActivity("root") {
            do {
                try await markStallRegion("outer") {
                    try await markStallRegion("inner") {
                        throw TestError()
                    }
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first != nil)
            let steps = activities[0].steps
            try #require(steps.count == 2)

            // Outer (ancestor) has no detail
            #expect(steps[0].state == .failed(detail: nil))

            // Inner (deepest) has the error message
            if case let .failed(detail: innerDetail) = steps[1].state {
                #expect(innerDetail != nil)
            } else {
                Issue.record("inner step should be failed with detail")
            }
        }
    }

    @Test("independent sibling failures each keep their message")
    func independentSiblingFailuresKeepMessages() async throws {
        let report = StalenessReport()

        struct FirstError: Error {}
        struct SecondError: Error {}

        try await report.trackActivity("root") {
            do {
                try await markStallRegion("first") {
                    throw FirstError()
                }
            } catch {}

            do {
                try await markStallRegion("second") {
                    throw SecondError()
                }
            } catch {}

            await report.drain()
            let activities = report.subject.value
            try #require(activities.first != nil)
            let steps = activities[0].steps
            try #require(steps.count == 2)

            // Both siblings failed independently (neither is ancestor of the other)
            // so both should carry their own error messages
            if case let .failed(detail: firstDetail) = steps[0].state {
                #expect(firstDetail != nil)
            } else {
                Issue.record("first step should be failed with detail")
            }

            if case let .failed(detail: secondDetail) = steps[1].state {
                #expect(secondDetail != nil)
            } else {
                Issue.record("second step should be failed with detail")
            }
        }
    }

    @Test("successful root activity survives with endedAt set")
    func successfulRootActivitySurvivesWithEndedAt() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            await markStallRegion("child") {}
        }

        await report.drain()
        let activities = report.subject.value
        #expect(activities.count == 1)
        #expect(activities[0].title == "root")
        #expect(activities[0].endedAt != nil)
        #expect(activities[0].steps.allSatisfy { $0.state == .finished })
    }

    @Test("dismiss after end removes activity")
    func dismissAfterEndRemovesActivity() async throws {
        let report = StalenessReport()

        await report.trackActivity("root") {
            await markStallRegion("child") {}
        }

        await report.drain()
        let activities = report.subject.value
        let activityId = try #require(activities.first?.id)

        await report.dismiss(id: activityId)

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }

    @Test("dismiss while running then end removes activity")
    func dismissWhileRunningThenEndRemovesActivity() async throws {
        let report = StalenessReport()

        let activityId = Ref<UUID?>(nil)
        var task: Task<Void, Error>?

        let activityStarted = AsyncCurrentValueSubject<Bool>(false)

        task = Task {
            try await report.trackActivity("root") {
                if let handle = StalenessDiagnostics.collector as? StallHandle {
                    activityId.value = handle.id
                }
                activityStarted.value = true
                try await Task.sleep(for: .seconds(10))
            }
        }

        for await _ in activityStarted where activityStarted.value {
            break
        }

        let id = try #require(activityId.value)
        await report.dismiss(id: id)

        task?.cancel()
        _ = try? await task?.value

        await report.drain()
        #expect(report.subject.value.isEmpty)
    }
}

/// These exercise `StalenessReport.shared` and the `isEnabled` global, so they must not run
/// concurrently with each other — Swift Testing parallelises within a suite by default.
@Suite("markStallActivity", .serialized)
struct MarkStallActivityTests {
    @Test("markStallActivity publishes nothing while disabled")
    func markStallActivityDisabledPublishesNothing() async throws {
        defer { StalenessReport.isEnabled = false }
        StalenessReport.isEnabled = false

        var bodyRan = false
        await markStallActivity("root") {
            bodyRan = true
        }

        #expect(bodyRan)
        await StalenessReport.shared.drain()
        #expect(StalenessReport.shared.subject.value.isEmpty)
    }

    @Test("markStallActivity publishes an activity while enabled")
    func markStallActivityEnabledPublishesActivity() async throws {
        defer { StalenessReport.isEnabled = false }
        StalenessReport.isEnabled = true

        try await markStallActivity("root") {
            await StalenessReport.shared.drain()
            let activities = StalenessReport.shared.subject.value
            #expect(activities.count == 1)
            try #require(activities.first?.title == "root")
        }

        await StalenessReport.shared.drain()
        let activityId = try #require(StalenessReport.shared.subject.value.first?.id)
        await StalenessReport.shared.dismiss(id: activityId)
        await StalenessReport.shared.drain()
    }

    @Test("the disabled path rethrows unchanged")
    func markStallActivityDisabledRethrowsUnchanged() async throws {
        defer { StalenessReport.isEnabled = false }
        StalenessReport.isEnabled = false

        struct TestError: Error {}

        var errorWasThrown = false
        do {
            try await markStallActivity("root") {
                throw TestError()
            }
        } catch is TestError {
            errorWasThrown = true
        }

        #expect(errorWasThrown)
        await StalenessReport.shared.drain()
        #expect(StalenessReport.shared.subject.value.isEmpty)
    }

    @Test("markStallRegion outside an activity records nothing")
    func markStallRegionOutsideActivityRecordsNothing() async throws {
        defer { StalenessReport.isEnabled = false }
        StalenessReport.isEnabled = false

        await markStallRegion("orphan") {}

        await StalenessReport.shared.drain()
        #expect(StalenessReport.shared.subject.value.isEmpty)
    }
}

private class Ref<T> {
    var value: T

    init(_ value: T) {
        self.value = value
    }
}

private func awaitIngestion(
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
            throw AwaitIngestionTimeout()
        }

        _ = try await group.next()
        group.cancelAll()
    }
}

private struct AwaitIngestionTimeout: Error {}
