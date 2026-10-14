import Testing
@testable import BackgroundExecution

@MainActor
struct BackgroundExecutorTests {
    @Test func returnsOperationValueAndPairsBeginEnd() async throws {
        let host = BackgroundTaskHostMock()
        let executor = BackgroundExecutor(host: host)

        let result = try await executor.execute { 42 }

        #expect(result == 42)
        #expect(host.beginCallCount == 1)
        #expect(host.endCallCount == 1)
    }

    @Test func propagatesOperationErrorAndStillEndsTask() async throws {
        let host = BackgroundTaskHostMock()
        let executor = BackgroundExecutor(host: host)

        await #expect(throws: SampleError.self) {
            try await executor.execute { throw SampleError() }
        }

        #expect(host.beginCallCount == 1)
        #expect(host.endCallCount == 1)
    }

    @Test func expirationCancelsOperationAndSurfacesExpiredError() async throws {
        let host = BackgroundTaskHostMock()
        let executor = BackgroundExecutor(host: host)

        let task = Task { () -> Int in
            try await executor.execute { () -> Int in
                while true {
                    try Task.checkCancellation()
                    await Task.yield()
                }
            }
        }

        while host.beginCallCount == 0 {
            await Task.yield()
        }
        host.fireExpiration()

        await #expect(throws: BackgroundExecutionExpiredError.self) {
            try await task.value
        }
        #expect(host.beginCallCount == 1)
        #expect(host.endCallCount == 1)
    }

    @Test func outerCancellationSurfacesCancellationErrorNotExpired() async throws {
        let host = BackgroundTaskHostMock()
        let executor = BackgroundExecutor(host: host)

        let task = Task { () -> Int in
            try await executor.execute { () -> Int in
                while true {
                    try Task.checkCancellation()
                    await Task.yield()
                }
            }
        }

        while host.beginCallCount == 0 {
            await Task.yield()
        }
        task.cancel()

        await #expect(throws: CancellationError.self) {
            try await task.value
        }
        #expect(host.endCallCount == 1)
    }

    @Test func invalidTokenRunsOperationAndSkipsEnd() async throws {
        let host = BackgroundTaskHostMock(token: .invalid)
        let executor = BackgroundExecutor(host: host)

        let result = try await executor.execute { 7 }

        #expect(result == 7)
        #expect(host.beginCallCount == 1)
        #expect(host.endCallCount == 0)
    }
}
