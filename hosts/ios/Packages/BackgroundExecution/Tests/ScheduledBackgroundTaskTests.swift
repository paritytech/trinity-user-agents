import Foundation
import os
import Testing
import FoundationExtTestSupport
@testable import BackgroundExecution

struct ScheduledBackgroundTaskTests {
    private static let identifier = "io.test.statement-renewal"
    private static let now = Date(timeIntervalSince1970: 1_000_000)
    private static let constraints = BackgroundTaskConstraints(
        requiresNetworkConnectivity: true,
        requiresExternalPower: false
    )
    private static let policy = BackgroundTaskRequestPolicy(earliestBeginDelay: 3_600, constraints: constraints)

    @Test func registersOnlyOnceWhenCalledRepeatedly() {
        let scheduler = BackgroundTaskSchedulerSpy()
        let task = makeTask(scheduler: scheduler) { .completed(logFields: "") }

        task.register()
        task.register()

        #expect(scheduler.registeredIdentifiers == [Self.identifier])
    }

    @Test func logsRejectedRegistration() {
        let scheduler = BackgroundTaskSchedulerSpy(acceptsRegistration: false)
        let logger = RecordingLogger()
        let task = makeTask(scheduler: scheduler, logger: logger) { .completed(logFields: "") }

        task.register()

        #expect(logger.messages.contains { $0.hasPrefix("registerRejected") })
    }

    @Test func scheduleSubmitsPolicyRequestDelayedFromNow() async {
        let scheduler = BackgroundTaskSchedulerSpy()
        let task = makeTask(scheduler: scheduler) { .completed(logFields: "") }

        await task.schedule()

        let expected = BackgroundTaskRequest(
            identifier: Self.identifier,
            earliestBeginDate: Self.now.addingTimeInterval(3_600),
            constraints: Self.constraints
        )
        #expect(scheduler.submittedRequests == [expected])
    }

    @Test func requestIsPendingOnlyAfterSchedule() async {
        let task = makeTask { .completed(logFields: "") }

        let before = await task.isRequestPending()
        await task.schedule()
        let after = await task.isRequestPending()

        #expect(!before)
        #expect(after)
    }

    @Test func submitFailureIsLoggedNotThrown() async {
        let scheduler = BackgroundTaskSchedulerSpy(submitError: SampleError())
        let logger = RecordingLogger()
        let task = makeTask(scheduler: scheduler, logger: logger) { .completed(logFields: "") }

        await task.schedule()

        #expect(scheduler.submittedRequests.isEmpty)
        #expect(logger.messages.contains { $0.hasPrefix("submitFailed") })
    }

    @Test func launchSubmitsNextRequestBeforeWorkStarts() async {
        let scheduler = BackgroundTaskSchedulerSpy()
        let submittedAtWorkStart = OSAllocatedUnfairLock<Int?>(initialState: nil)
        let task = makeTask(scheduler: scheduler) {
            submittedAtWorkStart.withLock { $0 = scheduler.submittedRequests.count }
            return .completed(logFields: "")
        }

        await task.handle(BackgroundTaskHandleSpy()).value

        #expect(submittedAtWorkStart.withLock { $0 } == 1)
    }

    @Test func completedWorkCompletesTaskOnceWithSuccess() async {
        let handle = BackgroundTaskHandleSpy()
        let logger = RecordingLogger()
        let task = makeTask(logger: logger) { .completed(logFields: "renewed=3") }

        await task.handle(handle).value

        #expect(handle.completions == [true])
        #expect(logger.messages.contains { $0.hasSuffix("outcome=completed renewed=3") })
    }

    @Test func skippedWorkCompletesTaskWithSuccess() async {
        let handle = BackgroundTaskHandleSpy()
        let task = makeTask { .skipped(reason: "notReady") }

        await task.handle(handle).value

        #expect(handle.completions == [true])
    }

    @Test func throwingWorkCompletesTaskWithFailure() async {
        let handle = BackgroundTaskHandleSpy()
        let logger = RecordingLogger()
        let task = makeTask(logger: logger) { throw SampleError() }

        await task.handle(handle).value

        #expect(handle.completions == [false])
        #expect(logger.messages.contains { $0.hasSuffix("outcome=failed error=SampleError") })
    }

    @Test func expiryCancelsWorkAndCompletesTaskOnceWithFailure() async {
        let handle = BackgroundTaskHandleSpy()
        let started = AsyncStream<Void>.makeStream()
        let task = makeTask {
            started.continuation.yield()
            while !Task.isCancelled {
                await Task.yield()
            }
            return .completed(logFields: "")
        }

        let run = task.handle(handle)
        for await _ in started.stream {
            break
        }
        handle.fireExpiration()
        await run.value

        #expect(handle.completions == [false])
    }

    @Test func expiryRacingFinishedWorkCompletesTaskOnce() async {
        let handle = BackgroundTaskHandleSpy()
        let task = makeTask { .completed(logFields: "") }

        let run = task.handle(handle)
        handle.fireExpiration()
        await run.value

        #expect(handle.completions.count == 1)
    }
}

private extension ScheduledBackgroundTaskTests {
    func makeTask(
        scheduler: BackgroundTaskSchedulerSpy = BackgroundTaskSchedulerSpy(),
        logger: RecordingLogger = RecordingLogger(),
        work: @escaping BackgroundTaskWork
    ) -> ScheduledBackgroundTask {
        ScheduledBackgroundTask(
            identifier: Self.identifier,
            policy: Self.policy,
            dependencies: ScheduledBackgroundTaskDependencies(
                scheduler: scheduler,
                dateProvider: FixedDateProvider(Self.now),
                logger: logger
            ),
            work: work
        )
    }
}
