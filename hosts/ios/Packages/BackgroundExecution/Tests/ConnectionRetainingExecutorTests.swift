import Testing
@testable import BackgroundExecution

struct ConnectionRetainingExecutorTests {
    @Test func retainsAllOnceBeforeRunningOperation() async throws {
        let spy = SpyRetentionProvider()
        let executor = ConnectionRetainingExecutor(
            provider: spy,
            innerExecutor: InlineExecutor()
        )

        let retainsSeenWhenOperationStarted = try await executor.execute {
            spy.allScopeRetainCount
        }

        #expect(retainsSeenWhenOperationStarted == 1)
        #expect(spy.totalRetainCount == 1)
    }

    @Test func propagatesOperationErrorAfterRetaining() async throws {
        let spy = SpyRetentionProvider()
        let executor = ConnectionRetainingExecutor(
            provider: spy,
            innerExecutor: InlineExecutor()
        )

        await #expect(throws: SampleError.self) {
            try await executor.execute { throw SampleError() }
        }

        #expect(spy.totalRetainCount == 1)
    }
}
