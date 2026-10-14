import Testing
@testable import CommonService

struct BaseObservableStoreAsyncTests {
    struct TestState: Equatable {
        let value: Int
    }

    /// Replaces the wall-clock `Task.sleep` that only *hoped* a subscription had established before the
    /// producer mutated. Each observer calls ``signalReady()`` once it has received its initial buffered
    /// state — which proves `observe()` has registered it — and the mutator awaits ``waitForReady(count:)``
    /// before updating. Delivery is then deterministic instead of racing a fixed delay.
    struct ReadinessBarrier: Sendable {
        private let stream: AsyncStream<Void>
        private let continuation: AsyncStream<Void>.Continuation

        init() {
            (stream, continuation) = AsyncStream<Void>.makeStream()
        }

        func signalReady() { continuation.yield(()) }

        func waitForReady(count: Int) async {
            var seen = 0
            for await _ in stream {
                seen += 1
                if seen >= count { return }
            }
        }
    }

    @Test("Observe receives initial nil state")
    func observeReceivesInitialNilState() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())

        // When
        let stream = store.observe()
        var receivedStates: [TestState?] = []

        // Then - should receive initial nil state
        for try await state in stream {
            receivedStates.append(state)
            break
        }

        #expect(receivedStates.count == 1)

        let actualValue: TestState? = receivedStates.first as? TestState
        #expect(actualValue == nil)
    }

    @Test("Observe receives current state on subscription")
    func observeReceivesCurrentState() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let expectedState = TestState(value: 42)
        store.updateState(expectedState)

        // When
        let stream = store.observe()
        var receivedStates: [TestState?] = []

        // Then - should receive current state immediately
        for try await state in stream {
            receivedStates.append(state)
            break
        }

        #expect(receivedStates.count == 1)
        #expect(receivedStates.first == expectedState)
    }

    @Test("Observe receives state updates")
    func observeReceivesStateUpdates() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let expectedStates = [
            TestState(value: 1),
            TestState(value: 2),
            TestState(value: 3)
        ]

        // When
        let stream = store.observe()
        var receivedStates: [TestState?] = []
        let readiness = ReadinessBarrier()

        let task = Task {
            for try await state in stream {
                receivedStates.append(state)
                if receivedStates.count == 1 { readiness.signalReady() }
                // Initial nil + 3 updates
                if receivedStates.count >= 4 {
                    break
                }
            }
        }

        // Wait for the subscription to establish (initial state received)
        await readiness.waitForReady(count: 1)

        // Update states — buffered in order, so no inter-update delay is needed
        for state in expectedStates {
            store.updateState(state)
        }

        try await task.value

        // Then
        #expect(receivedStates.count == 4)
        #expect(receivedStates[0] == nil) // Initial nil state
        #expect(receivedStates[1] == expectedStates[0])
        #expect(receivedStates[2] == expectedStates[1])
        #expect(receivedStates[3] == expectedStates[2])
    }

    @Test("Multiple observers receive updates")
    func multipleObserversReceiveUpdates() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let testState = TestState(value: 100)

        // When - create two observers
        let stream1 = store.observe()
        let stream2 = store.observe()

        var observer1States: [TestState?] = []
        var observer2States: [TestState?] = []
        let readiness = ReadinessBarrier()

        let task1 = Task {
            for try await state in stream1 {
                observer1States.append(state)
                if observer1States.count == 1 { readiness.signalReady() }
                if observer1States.count >= 2 {
                    break
                }
            }
        }

        let task2 = Task {
            for try await state in stream2 {
                observer2States.append(state)
                if observer2States.count == 1 { readiness.signalReady() }
                if observer2States.count >= 2 {
                    break
                }
            }
        }

        // Wait for both subscriptions to establish
        await readiness.waitForReady(count: 2)

        store.updateState(testState)

        try await task1.value
        try await task2.value

        // Then - both observers should receive both states
        #expect(observer1States.count == 2)
        #expect(observer2States.count == 2)

        #expect(observer1States[0] == nil)
        #expect(observer1States[1] == testState)

        #expect(observer2States[0] == nil)
        #expect(observer2States[1] == testState)
    }

    @Test("Observer is removed on stream cancellation")
    func observerIsRemovedOnStreamCancellation() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())

        // When - create and cancel a task
        let task = Task {
            let stream = store.observe()
            for try await _ in stream {
                break
            }
        }

        try await task.value

        // Then - a later update still lands on the store's own state, and the removed observer's
        // terminated continuation quietly drops it rather than crashing.
        store.updateState(TestState(value: 999))

        #expect(store.currentState == TestState(value: 999))
    }

    @Test("State transitions are delivered in order")
    func stateTransitionsAreDeliveredInOrder() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let stateSequence = (1 ... 10).map { TestState(value: $0) }

        // When
        let stream = store.observe()
        var receivedStates: [TestState?] = []
        let readiness = ReadinessBarrier()

        let task = Task {
            for try await state in stream {
                receivedStates.append(state)
                if receivedStates.count == 1 { readiness.signalReady() }
                // Initial nil + 10 updates
                if receivedStates.count >= 11 {
                    break
                }
            }
        }

        // Wait for the subscription to establish (initial state received)
        await readiness.waitForReady(count: 1)

        // Rapidly update states
        for state in stateSequence {
            store.updateState(state)
        }

        try await task.value

        // Then
        #expect(receivedStates.count == 11)
        #expect(receivedStates[0] == nil)

        // Verify order
        for (index, expectedState) in stateSequence.enumerated() {
            #expect(receivedStates[index + 1] == expectedState)
        }
    }

    @Test("Nil state updates are delivered")
    func nilStateUpdatesAreDelivered() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let nonNilState = TestState(value: 50)
        store.updateState(nonNilState)

        // When
        let stream = store.observe()
        var receivedStates: [TestState?] = []
        let readiness = ReadinessBarrier()

        let task = Task {
            for try await state in stream {
                receivedStates.append(state)
                if receivedStates.count == 1 { readiness.signalReady() }
                if receivedStates.count >= 2 {
                    break
                }
            }
        }

        // Wait for the subscription to establish (current state received)
        await readiness.waitForReady(count: 1)

        // Update to nil
        store.updateState(nil)

        try await task.value

        // Then
        #expect(receivedStates.count == 2)
        #expect(receivedStates[0] == nonNilState) // Current state at subscription
        #expect(receivedStates[1] == nil) // Nil update
    }

    @Test("Concurrent subscriptions work correctly")
    func concurrentSubscriptionsWork() async throws {
        // Given
        let store = MockObservableStore<TestState>(logger: MockLogger())
        let concurrentCount = 5
        let readiness = ReadinessBarrier()

        // When - create multiple concurrent subscriptions
        var tasks: [Task<[TestState?], any Error>] = []

        for _ in 0 ..< concurrentCount {
            let task = Task<[TestState?], any Error> {
                var states: [TestState?] = []
                let stream = store.observe()

                for try await state in stream {
                    states.append(state)
                    if states.count == 1 { readiness.signalReady() }
                    if states.count >= 2 {
                        break
                    }
                }
                return states
            }
            tasks.append(task)
        }

        // Wait for every subscription to establish
        await readiness.waitForReady(count: concurrentCount)

        // Trigger update
        store.updateState(TestState(value: 777))

        // Collect results
        var allResults: [[TestState?]] = []
        for task in tasks {
            let result = try await task.value
            allResults.append(result)
        }

        // Then - all observers should have received updates
        for (index, results) in allResults.enumerated() {
            #expect(results.count == 2, "Observer \(index) should have 2 states")
            #expect(results[0] == nil, "Observer \(index) first state should be nil")
            #expect(results[1] == TestState(value: 777), "Observer \(index) second state mismatch")
        }
    }
}
