@testable import polkadot_app
import NovaCrypto
import MessageExchangeKit
import CryptoKit
import Testing
import StructuredConcurrencyTestSupport
import SubstrateSdk
import AsyncExtensions
import os
import CustomDump

private enum TestTrackingError: Error {
    case simulated
}

@Suite(.timeLimit(.minutes(1)))
struct FiatOnrampTransactionTrackingTests {
    static let constantDate = Date()
    private let expectedAmountIn = Balance(10)
    private let expectedAmountOut = Balance(20)

    @Test func emmitsTransactionStatusOnDiscovery() async throws {
        let sessionId1 = FiatOnRampSessionId(value: UUID().uuidString)
        let sessionId2 = FiatOnRampSessionId(value: UUID().uuidString)

        let transaction1 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-1"),
            sessionId: sessionId1,
            status: .pending
        )
        let transaction2 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-2"),
            sessionId: sessionId2,
            status: .settling
        )

        let testStorage = TestFiatOnrampStorage()
        let testService = TestFiatOnrampService()
        await testStorage.addSessionId(sessionId1)
        await testStorage.addSessionId(sessionId2)
        await testService.setStubbedSessionTransactions([transaction1, transaction2])
        var spec = makeSpec(storage: testStorage, fiatService: testService)
        defer { spec.teardown() }
        try await spec.setup()

        _ = try await spec.nextStatuses()
        let emittedStatuses = try await spec.nextStatuses()

        expectNoDifference(
            emittedStatuses,
            [
                .init(id: transaction1.transactionId, status: .funding),
                .init(id: transaction2.transactionId, status: .funding)
            ]
        )
    }

    @Test func startTrackingPersistsSessionAndDiscoversOnTick() async throws {
        let sessionId = FiatOnRampSessionId(value: UUID().uuidString)
        let transaction = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-start-tracking"),
            sessionId: sessionId,
            status: .pending
        )

        let testStorage = TestFiatOnrampStorage()
        let testService = TestFiatOnrampService()
        await testService.setStubbedSessionTransactions([transaction])

        var spec = makeSpec(storage: testStorage, fiatService: testService)
        defer { spec.teardown() }
        try await spec.setup()

        let initial = try await spec.nextStatuses()
        #expect(initial.isEmpty)
        let initialFetchRequests = await testService.fetchTransactionsRequestsSnapshot()
        #expect(initialFetchRequests.isEmpty)

        spec.sut.startTracking(sessionId: sessionId)

        await spec.tick()

        let discovered = try await spec.nextStatuses()
        expectNoDifference(
            discovered,
            [
                .init(id: transaction.transactionId, status: .funding)
            ]
        )

        let fetchRequests = await testService.fetchTransactionsRequestsSnapshot()
        #expect(
            fetchRequests.contains { request in
                request.sessionIds == Set([sessionId])
            }
        )
        let storedSessionIds = await spec.storage.getSessionIds()
        #expect(!storedSessionIds.contains(sessionId))
    }

    @Test func handleBuySuccessDiscoversOnlyRequestedSession() async throws {
        let sessionId1 = FiatOnRampSessionId(value: UUID().uuidString)
        let sessionId2 = FiatOnRampSessionId(value: UUID().uuidString)
        let tx1 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-handle-1"),
            sessionId: sessionId1,
            status: .pending
        )
        let tx2 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-handle-2"),
            sessionId: sessionId2,
            status: .pending
        )

        let testStorage = TestFiatOnrampStorage()
        let testService = TestFiatOnrampService()
        await testService.setStubbedSessionTransactions([tx1, tx2])

        var spec = makeSpec(storage: testStorage, fiatService: testService)
        defer { spec.teardown() }
        try await spec.setup()

        _ = try await spec.nextStatuses()

        spec.sut.startTracking(sessionId: sessionId1)
        spec.sut.startTracking(sessionId: sessionId2)
        spec.sut.handleBuySuccess(for: sessionId1)

        let discovered = try await spec.nextStatuses()
        expectNoDifference(
            discovered,
            [
                .init(id: tx1.transactionId, status: .funding)
            ]
        )

        let fetchRequests = await testService.fetchTransactionsRequestsSnapshot()
        #expect(
            fetchRequests.contains { request in
                request.sessionIds == Set([sessionId1])
            }
        )
        let storedSessionIds = await spec.storage.getSessionIds()
        #expect(!storedSessionIds.contains(sessionId1))
        #expect(storedSessionIds.contains(sessionId2))
    }

    @Test func discoveryDoesNotDuplicateAlreadyTrackedTransactions() async throws {
        let sessionId = FiatOnRampSessionId(value: UUID().uuidString)
        let existingId = FiatOnRampTransactionId(value: "tx-existing")
        let newId = FiatOnRampTransactionId(value: "tx-new")

        let existingTracked = FiatOnrampTrackedTransaction(
            id: existingId,
            status: .funding(.inProgress),
            lastUpdate: Self.constantDate.timeIntervalSince1970
        )

        let duplicate = FiatOnrampTransactionSummary(
            transactionId: existingId,
            sessionId: sessionId,
            status: .pending
        )
        let discovered = FiatOnrampTransactionSummary(
            transactionId: newId,
            sessionId: sessionId,
            status: .pending
        )

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([existingTracked])
        let testService = TestFiatOnrampService()
        await testService.setStubbedSessionTransactions([duplicate, discovered])

        var spec = makeSpec(storage: testStorage, fiatService: testService)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.sut.startTracking(sessionId: sessionId)
        spec.sut.handleBuySuccess(for: sessionId)

        let nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(id: existingId, status: .funding),
                .init(id: newId, status: .funding)
            ]
        )

        let trackedTransactions = await spec.storage.getTrackedTransactions()
        #expect(trackedTransactions.count == 2)
        #expect(trackedTransactions.contains(where: { $0.id == existingId }))
        #expect(trackedTransactions.contains(where: { $0.id == newId }))
    }

    @Test func pollAndEmmitsNewTransactionStatuses() async throws {
        // Given: Storage contains session IDs
        let sessionId1 = FiatOnRampSessionId(value: UUID().uuidString)
        let sessionId2 = FiatOnRampSessionId(value: UUID().uuidString)
        let testStorage = TestFiatOnrampStorage()

        // And: Service returns transactions for those sessions
        let transaction1 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-1"),
            sessionId: sessionId1,
            status: .pending
        )
        let transaction2 = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-2"),
            sessionId: sessionId2,
            status: .settling
        )

        let testService = TestFiatOnrampService()

        await testStorage.addTrackedTransactions([
            makeTrackedTransaction(id: transaction1.transactionId.value, status: .funding(.inProgress)),
            makeTrackedTransaction(id: transaction2.transactionId.value, status: .funding(.inProgress))
        ])
        await testService.setStubbedTransactionSummaries([transaction1, transaction2])

        var spec = makeSpec(storage: testStorage, fiatService: testService)
        defer { spec.teardown() }
        try await spec.setup()

        // The initial snapshot, then the one from the poll that runs at setup.
        _ = try await spec.nextStatuses()
        let fetchedStatuses = try await spec.nextStatuses()

        expectNoDifference(
            fetchedStatuses,
            [
                .init(id: transaction1.transactionId, status: .funding),
                .init(id: transaction2.transactionId, status: .funding)
            ]
        )

        // And: Update the stubbed responses with new statuses
        let updatedTransaction1 = FiatOnrampTransactionSummary(
            transactionId: transaction1.transactionId,
            sessionId: sessionId1,
            status: .settled
        )
        let updatedTransaction2 = FiatOnrampTransactionSummary(
            transactionId: transaction2.transactionId,
            sessionId: sessionId2,
            status: .failed
        )
        await testService.setStubbedTransactionSummaries([updatedTransaction1, updatedTransaction2])

        // When: The polling interval elapses
        await spec.tick()

        // Then: Updated transaction statuses are emitted
        let updatedStatuses = try await spec.nextStatuses()
        expectNoDifference(
            updatedStatuses,
            [
                .init(id: transaction1.transactionId, status: .funding),
                .init(id: transaction2.transactionId, status: .failed)
            ]
        )
    }

    @Test func autoSwapStarted() async throws {
        try await assertAutoSwapUpdatesMatchingTransactionStatusAccordingly(
            from: .funding(.completed),
            for: .pendingSwap(expectedExecutionTime: 80),
            expectedStatus: .inProgress(remainingTime: 80)
        )
    }

    @Test func autoSwapIgnoresNonFiatOnrampFundedAssetExecution() async throws {
        let testStorage = TestFiatOnrampStorage()
        let tracked = makeTrackedTransaction(id: "tx-non-fiat-onramp-asset", status: .funding(.completed))
        await testStorage.addTrackedTransactions([tracked])

        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.depositService.emitExecutions(
            [makeExecution(
                label: makeNonFiatOnrampExecLabel(balance: Balance(100)),
                status: .pendingSwap(expectedExecutionTime: 80)
            )]
        )
        // Executions are handled in order, so the snapshot of this unmatched one follows the ignored batch.
        spec.depositService.emitExecutions(
            [makeExecution(label: makeExecLabel(balance: Balance(999)), status: .inProgress(remainedTime: 30))]
        )

        let statusesAfterExecutions = try await spec.nextStatuses()
        expectNoDifference(statusesAfterExecutions, [.init(id: tracked.id, status: .funding)])

        let storedTransactions = await spec.storage.getTrackedTransactions()
        expectNoDifference(storedTransactions, [tracked])
    }

    @Test func autoSwapUpdatesProgress() async throws {
        let execLabel = makeExecLabel(balance: Balance(100))
        try await assertAutoSwapUpdatesMatchingTransactionStatusAccordingly(
            from: .swapping(.init(
                status: .inProgress(remainingTime: 80),
                swapLabel: execLabel,
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            )),
            for: .inProgress(remainedTime: 60),
            expectedStatus: .inProgress(remainingTime: 60)
        )
    }

    @Test func autoSwapCompleted() async throws {
        let execLabel = makeExecLabel(balance: Balance(100))
        try await assertAutoSwapUpdatesMatchingTransactionStatusAccordingly(
            from: .swapping(.init(
                status: .inProgress(remainingTime: 80),
                swapLabel: execLabel,
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            )),
            for: .completed(receivedAmount: 100),
            expectedStatus: .completed
        )
    }

    @Test func autoSwapFailed() async throws {
        let execLabel = makeExecLabel(balance: Balance(100))
        try await assertAutoSwapUpdatesMatchingTransactionStatusAccordingly(
            from: .swapping(.init(
                status: .inProgress(remainingTime: 80),
                swapLabel: execLabel,
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            )),
            for: .failed,
            expectedStatus: .failed
        )
    }

    func assertAutoSwapUpdatesMatchingTransactionStatusAccordingly(
        from currentStatus: FiatOnrampTrackedTransactionStatus,
        for executionStatus: DepositExecutionItem.Status,
        expectedStatus: FiatOnrampTrackedTransactionStatus.Swapping.Status
    ) async throws {
        // Given: Storage contains tracked transactions
        let testStorage = TestFiatOnrampStorage()
        let execLabel = makeExecLabel(balance: Balance(100))

        let funded = makeTrackedTransaction(id: "tx-funded", status: currentStatus)
        await testStorage.addTrackedTransactions([funded])

        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()

        var emittedStatuses = try await spec.nextStatuses()

        expectNoDifference(
            emittedStatuses,
            [
                .init(id: funded.id, status: .init(trackedTransactionStatus: currentStatus)),
            ]
        )

        spec.depositService.emitExecutions(
            [makeExecution(label: execLabel, status: executionStatus)]
        )

        emittedStatuses = try await spec.nextStatuses()
        let expectTrackedTransaction = makeTrackedTransaction(
            id: funded.id.value,
            status: .swapping(.init(
                status: expectedStatus,
                swapLabel: execLabel,
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            ))
        )

        expectNoDifference(
            emittedStatuses,
            [
                .init(id: funded.id, status: .init(trackedTransactionStatus: expectTrackedTransaction.status))
            ]
        )

        let storedTransactions = await spec.storage.getTrackedTransactions()
        expectNoDifference(storedTransactions, [expectTrackedTransaction])
    }

    @Test func autoSwapInProgressIgnoresMismatchedLabel() async throws {
        let trackedLabel = makeExecLabel(balance: Balance(100))
        let executionLabel = makeExecLabel(balance: Balance(200))
        let trackedAmountIn = Balance(111)
        let trackedAmountOut = Balance(222)

        let tracked = makeTrackedTransaction(
            id: "tx-mismatch-label",
            status: .swapping(.init(
                status: .inProgress(remainingTime: 80),
                swapLabel: trackedLabel,
                amountIn: trackedAmountIn,
                amountOut: trackedAmountOut
            ))
        )

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([tracked])
        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.depositService.emitExecutions(
            [makeExecution(label: executionLabel, status: .inProgress(remainedTime: 30))]
        )

        let nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(
                    id: tracked.id,
                    status: .inProgress(
                        remainedTime: 80,
                        amountIn: trackedAmountIn,
                        amountOut: trackedAmountOut
                    )
                )
            ]
        )
    }

    @Test func autoSwapInProgressIgnoresTerminalSwaps() async throws {
        let label = makeExecLabel(balance: Balance(100))
        let trackedAmountIn = Balance(111)
        let trackedAmountOut = Balance(222)

        let tracked = makeTrackedTransaction(
            id: "tx-terminal-swap",
            status: .swapping(.init(
                status: .completed,
                swapLabel: label,
                amountIn: trackedAmountIn,
                amountOut: trackedAmountOut
            ))
        )

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([tracked])
        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.depositService.emitExecutions(
            [makeExecution(label: label, status: .inProgress(remainedTime: 30))]
        )

        let nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(
                    id: tracked.id,
                    status: .completed(amountIn: trackedAmountIn, amountOut: trackedAmountOut)
                )
            ]
        )
    }

    @Test func autoSwapPendingUpdatesMultipleFundedTransactions() async throws {
        let testStorage = TestFiatOnrampStorage()
        let execLabel = makeExecLabel(balance: Balance(1_100))

        let funded1 = makeTrackedTransaction(id: "tx-1", status: .funding(.completed))
        let funded2 = makeTrackedTransaction(id: "tx-2", status: .funding(.completed))
        await testStorage.addTrackedTransactions([funded1, funded2])

        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }

        try await spec.setup()
        let initialStatuses = try await spec.nextStatuses()
        expectNoDifference(
            initialStatuses,
            [
                .init(id: funded1.id, status: .funding),
                .init(id: funded2.id, status: .funding)
            ]
        )

        spec.depositService.emitExecutions(
            [makeExecution(label: execLabel, status: .pendingSwap(expectedExecutionTime: 75))]
        )

        let emittedStatuses = try await spec.nextStatuses()
        expectNoDifference(
            emittedStatuses,
            [
                .init(
                    id: funded1.id,
                    status: .inProgress(
                        remainedTime: 75,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(
                    id: funded2.id,
                    status: .inProgress(
                        remainedTime: 75,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                )
            ]
        )
    }

    @Test func concurrentAutoSwaps() async throws {
        let execLabelTx1 = makeExecLabel(balance: Balance(100))
        let execLabelTx2 = makeExecLabel(balance: Balance(120))
        let execLabelTx3 = makeExecLabel(balance: Balance(200))

        let testFiatService = TestFiatOnrampService()
        let testStorage = TestFiatOnrampStorage()
        let tx1 = makeTrackedTransaction(id: "tx-1", status: .funding(.completed))
        let tx2 = makeTrackedTransaction(id: "tx-2", status: .funding(.inProgress))
        let tx3 = makeTrackedTransaction(id: "tx-3", status: .funding(.inProgress))
        await testStorage.addTrackedTransactions([tx1, tx2, tx3])

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()

        let initialStatuses = try await spec.nextStatuses()
        let expectedInitialStatuses: Set<FiatOnrampTransactionStatusPayload> = [
            .init(id: tx1.id, status: .funding),
            .init(id: tx2.id, status: .funding),
            .init(id: tx3.id, status: .funding)
        ]
        expectNoDifference(initialStatuses, expectedInitialStatuses)

        spec.depositService.emitExecutions(
            [makeExecution(label: execLabelTx1, status: .pendingSwap(expectedExecutionTime: 60))]
        )

        let tx1SwapStartedStatuses = try await spec.nextStatuses()
        expectNoDifference(
            tx1SwapStartedStatuses,
            [
                .init(
                    id: tx1.id,
                    status: .inProgress(
                        remainedTime: 60,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(id: tx2.id, status: .funding),
                .init(id: tx3.id, status: .funding)
            ]
        )

        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: tx2.id, sessionId: .init(value: UUID().uuidString), status: .settled)
        ])
        await spec.tick()

        var nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(
                    id: tx1.id,
                    status: .inProgress(
                        remainedTime: 60,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(id: tx2.id, status: .funding),
                .init(id: tx3.id, status: .funding)
            ]
        )

        spec.depositService.emitExecutions(
            [
                makeExecution(label: execLabelTx1, status: .inProgress(remainedTime: 50)),
                makeExecution(label: execLabelTx2, status: .pendingSwap(expectedExecutionTime: 40))
            ]
        )

        nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(
                    id: tx1.id,
                    status: .inProgress(
                        remainedTime: 50,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(
                    id: tx2.id,
                    status: .inProgress(
                        remainedTime: 40,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(id: tx3.id, status: .funding)
            ]
        )

        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: tx3.id, sessionId: .init(value: UUID().uuidString), status: .settled)
        ])
        await spec.tick()

        nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(
                    id: tx1.id,
                    status: .inProgress(
                        remainedTime: 50,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(
                    id: tx2.id,
                    status: .inProgress(
                        remainedTime: 40,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                ),
                .init(id: tx3.id, status: .funding)
            ]
        )

        spec.depositService.emitExecutions(
            [
                makeExecution(label: execLabelTx1, status: .completed(receivedAmount: 100)),
                makeExecution(label: execLabelTx2, status: .failed),
                makeExecution(label: execLabelTx3, status: .pendingSwap(expectedExecutionTime: 10))
            ]
        )

        nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(id: tx1.id, status: .completed(amountIn: expectedAmountIn, amountOut: expectedAmountOut)),
                .init(id: tx2.id, status: .failed),
                .init(
                    id: tx3.id,
                    status: .inProgress(
                        remainedTime: 10,
                        amountIn: expectedAmountIn,
                        amountOut: expectedAmountOut
                    )
                )
            ]
        )
    }

    @Test func setupIsIdempotent() async throws {
        var spec = makeSpec()
        defer { spec.teardown() }
        try await spec.setup()

        let tasksBeforeSecondSetup = spec.observationTasks

        spec.sut.setup()

        #expect(spec.observationTasks == tasksBeforeSecondSetup)
    }

    @Test func throttleStopsPolling() async throws {
        let testStorage = TestFiatOnrampStorage()
        let testFiatService = TestFiatOnrampService()

        let tracked = makeTrackedTransaction(id: "tx-1", status: .funding(.inProgress))
        await testStorage.addTrackedTransactions([tracked])
        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: tracked.id, sessionId: .init(value: UUID().uuidString), status: .settling)
        ])

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        await spec.throttleAndWaitForTasks()
        await testFiatService.clearFetchTransactionRequests()

        let fetchRequestsAfterThrottle = await testFiatService.fetchTransactionRequestsSnapshot()
        #expect(fetchRequestsAfterThrottle.isEmpty)
    }

    @Test func throttleBeforeSetupIsNoop() async throws {
        let testStorage = TestFiatOnrampStorage()
        let testFiatService = TestFiatOnrampService()

        let tracked = makeTrackedTransaction(id: "tx-1", status: .funding(.inProgress))
        await testStorage.addTrackedTransactions([tracked])
        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: tracked.id, sessionId: .init(value: UUID().uuidString), status: .settling)
        ])

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        spec.sut.throttle()
        try await spec.setup()

        // The initial snapshot, then the one from the poll that runs at setup.
        _ = try await spec.nextStatuses()
        _ = try await spec.nextStatuses()

        let fetchRequestsAfterSetup = await testFiatService.fetchTransactionRequestsSnapshot()
        #expect(!fetchRequestsAfterSetup.isEmpty)
    }

    @Test func setupAfterThrottleRestartsPolling() async throws {
        let testStorage = TestFiatOnrampStorage()
        let testFiatService = TestFiatOnrampService()

        let tracked = makeTrackedTransaction(id: "tx-1", status: .funding(.inProgress))
        await testStorage.addTrackedTransactions([tracked])
        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: tracked.id, sessionId: .init(value: UUID().uuidString), status: .settling)
        ])

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()
        _ = try await spec.nextStatuses()

        await spec.throttleAndWaitForTasks()
        await testFiatService.clearFetchTransactionRequests()

        spec.sut.setup()
        await spec.waitForPeriodicTasks()

        // The initial snapshot, then the one from the poll that runs at setup.
        _ = try await spec.nextStatuses()
        _ = try await spec.nextStatuses()

        let fetchRequestsAfterRestart = await testFiatService.fetchTransactionRequestsSnapshot()
        #expect(!fetchRequestsAfterRestart.isEmpty)
    }

    @Test func removeFailedAndCompletedTransactionsEmitsUpdatedSnapshot() async throws {
        let failed = makeTrackedTransaction(id: "tx-failed", status: .funding(.failed))
        let completed = makeTrackedTransaction(
            id: "tx-completed",
            status: .swapping(.init(
                status: .completed,
                swapLabel: makeExecLabel(balance: Balance(100)),
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            ))
        )
        let inProgress = makeTrackedTransaction(id: "tx-progress", status: .funding(.inProgress))

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([failed, completed, inProgress])

        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.sut.removeFailedTransactions()
        let afterFailedRemoval = try await spec.nextStatuses()
        expectNoDifference(
            afterFailedRemoval,
            [
                .init(id: completed.id, status: .completed(amountIn: expectedAmountIn, amountOut: expectedAmountOut)),
                .init(id: inProgress.id, status: .funding)
            ]
        )

        spec.sut.removeCompletedTransactions()
        let afterCompletedRemoval = try await spec.nextStatuses()
        expectNoDifference(
            afterCompletedRemoval,
            [
                .init(id: inProgress.id, status: .funding)
            ]
        )
    }

    @Test func removeFailedTransactionsRemovesFailedSwaps() async throws {
        let failedSwap = makeTrackedTransaction(
            id: "tx-swap-failed",
            status: .swapping(.init(
                status: .failed,
                swapLabel: makeExecLabel(balance: Balance(100)),
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            ))
        )
        let inProgress = makeTrackedTransaction(id: "tx-progress", status: .funding(.inProgress))

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([failedSwap, inProgress])

        var spec = makeSpec(storage: testStorage)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        spec.sut.removeFailedTransactions()
        let nextStatuses = try await spec.nextStatuses()
        expectNoDifference(
            nextStatuses,
            [
                .init(id: inProgress.id, status: .funding)
            ]
        )
    }

    @Test func pollingFetchesOnlyFundingInProgressTransactions() async throws {
        let inProgressFunding = makeTrackedTransaction(id: "tx-in-progress", status: .funding(.inProgress))
        let completedFunding = makeTrackedTransaction(id: "tx-completed-funding", status: .funding(.completed))
        let failedFunding = makeTrackedTransaction(id: "tx-failed-funding", status: .funding(.failed))
        let swappingInProgress = makeTrackedTransaction(
            id: "tx-swapping",
            status: .swapping(.init(
                status: .inProgress(remainingTime: 30),
                swapLabel: makeExecLabel(balance: Balance(100)),
                amountIn: expectedAmountIn,
                amountOut: expectedAmountOut
            ))
        )

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([
            inProgressFunding, completedFunding, failedFunding, swappingInProgress
        ])
        let testFiatService = TestFiatOnrampService()
        await testFiatService.setStubbedTransactionSummaries([
            .init(
                transactionId: inProgressFunding.id,
                sessionId: .init(value: UUID().uuidString),
                status: .settling
            )
        ])
        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()

        // The initial snapshot, then the one from the poll that runs at setup.
        _ = try await spec.nextStatuses()
        _ = try await spec.nextStatuses()

        let fetchRequests = await testFiatService.fetchTransactionRequestsSnapshot()
        expectNoDifference(fetchRequests, [inProgressFunding.id])
    }

    @Test func pollingContinuesWhenOneFetchFails() async throws {
        let txSuccess = makeTrackedTransaction(id: "tx-success", status: .funding(.inProgress))
        let txFailure = makeTrackedTransaction(id: "tx-failure", status: .funding(.inProgress))

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addTrackedTransactions([txSuccess, txFailure])
        let testFiatService = TestFiatOnrampService()
        await testFiatService.setStubbedTransactionSummaries([
            .init(transactionId: txSuccess.id, sessionId: .init(value: UUID().uuidString), status: .failed),
            .init(transactionId: txFailure.id, sessionId: .init(value: UUID().uuidString), status: .settling)
        ])
        await testFiatService.setFetchTransactionFailingIds([txFailure.id])

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()
        _ = try await spec.nextStatuses()

        let nextStatuses = try await spec.nextStatuses()

        expectNoDifference(
            nextStatuses,
            [
                .init(id: txSuccess.id, status: .failed),
                .init(id: txFailure.id, status: .funding)
            ]
        )
    }

    @Test func trackingRecoversAfterDiscoverFailure() async throws {
        let sessionId = FiatOnRampSessionId(value: UUID().uuidString)
        let transaction = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-1"),
            sessionId: sessionId,
            status: .pending
        )

        let testStorage = TestFiatOnrampStorage()
        let testFiatService = TestFiatOnrampService()

        await testStorage.addSessionId(sessionId)
        await testFiatService.setStubbedSessionTransactions([transaction])
        await testFiatService.setFetchTransactionsFailuresLeft(1)

        var spec = makeSpec(storage: testStorage, fiatService: testFiatService)
        defer { spec.teardown() }
        try await spec.setup()

        let initial = try await spec.nextStatuses()
        #expect(initial.isEmpty)

        await spec.tick()

        let recoveredStatuses = try await spec.nextStatuses()
        expectNoDifference(
            recoveredStatuses,
            [
                .init(id: transaction.transactionId, status: .funding)
            ]
        )

        let fetchRequests = await testFiatService.fetchTransactionsRequestsSnapshot()
        #expect(fetchRequests.count == 2)
    }

    @Test func discoverTransactionsRemovesExpiredPendingSessions() async throws {
        let sessionTtl: TimeInterval = 4 * 60 * 60
        let now = Date(timeIntervalSince1970: 10_000)
        let expiredSessionId = FiatOnRampSessionId(value: UUID().uuidString)
        let activeSessionId = FiatOnRampSessionId(value: UUID().uuidString)
        let activeTransaction = FiatOnrampTransactionSummary(
            transactionId: FiatOnRampTransactionId(value: "tx-active"),
            sessionId: activeSessionId,
            status: .pending
        )

        let testStorage = TestFiatOnrampStorage()
        await testStorage.addSessionId(
            expiredSessionId,
            createdAt: now.timeIntervalSince1970 - sessionTtl - 1
        )
        await testStorage.addSessionId(
            activeSessionId,
            createdAt: now.timeIntervalSince1970 - sessionTtl + 60
        )

        let testFiatService = TestFiatOnrampService()
        await testFiatService.setStubbedSessionTransactions([activeTransaction])

        let spec = makeSpec(
            storage: testStorage,
            fiatService: testFiatService,
            dateBuilder: { now }
        )

        try await spec.sut.discoverTransactions()

        let fetchRequests = await testFiatService.fetchTransactionsRequestsSnapshot()
        #expect(fetchRequests.count == 1)
        #expect(fetchRequests.first?.sessionIds == Set([activeSessionId]))

        let storedSessionIds = await testStorage.getSessionIds()
        #expect(!storedSessionIds.contains(expiredSessionId))
        #expect(!storedSessionIds.contains(activeSessionId))
    }

    private func makeExecLabel(balance: Balance) -> DepositExecLabel {
        DepositExecLabel(chainAssetId: AppConfig.Assets.fiatOnrampFundedAsset, balance: balance)
    }

    private func makeNonFiatOnrampExecLabel(balance: Balance) -> DepositExecLabel {
        DepositExecLabel(chainAssetId: AppConfig.Assets.fundedAsset, balance: balance)
    }

    private func makeExecution(label: DepositExecLabel, status: DepositExecutionItem.Status) -> DepositExecutionItem {
        DepositExecutionItem(execLabel: label, amountIn: Balance(10), amountOut: Balance(20), status: status)
    }

    private func makeTrackedTransaction(
        id: String,
        status: FiatOnrampTrackedTransactionStatus
    ) -> FiatOnrampTrackedTransaction {
        FiatOnrampTrackedTransaction(
            id: FiatOnRampTransactionId(value: id),
            status: status,
            lastUpdate: Self.constantDate.timeIntervalSince1970
        )
    }

    private func makeSpec(
        storage: TestFiatOnrampStorage = TestFiatOnrampStorage(),
        depositService: TestDepositService = TestDepositService(),
        fiatService: TestFiatOnrampService = TestFiatOnrampService(),
        dateBuilder: @escaping () -> Date = { Self.constantDate }
    ) -> TrackingSpec {
        TrackingSpec(
            storage: storage,
            depositService: depositService,
            fiatService: fiatService,
            dateBuilder: dateBuilder
        )
    }
}

private struct TrackingSpec {
    let storage: TestFiatOnrampStorage
    let depositService: TestDepositService
    let fiatService: TestFiatOnrampService
    let clock = ManualClock()
    let sut: FiatOnrampTrackingServicing

    private var iterator: AnyAsyncSequence<Set<FiatOnrampTransactionStatusPayload>>.AsyncIterator?
    private var statusStream: AnyAsyncSequence<Set<FiatOnrampTransactionStatusPayload>>?

    var observationTasks: [Task<Void, Never>?] {
        [sut.updateTriggersTask, sut.sessionDiscoveryTask, sut.transactionPollingTask, sut.autoSwapDepositsTask]
    }

    init(
        storage: TestFiatOnrampStorage,
        depositService: TestDepositService,
        fiatService: TestFiatOnrampService,
        dateBuilder: @escaping () -> Date
    ) {
        self.storage = storage
        self.depositService = depositService
        self.fiatService = fiatService
        sut = FiatOnrampTrackingServicing(
            depositService: depositService,
            fiatOnrampService: fiatService,
            fiatOnrampStorage: storage,
            clock: clock,
            dateBuilder: dateBuilder
        )
    }

    mutating func subscribe() async {
        statusStream = await sut.subscribeToTransactionStatuses()
        iterator = statusStream?.makeAsyncIterator()
    }

    /// Subscribing first makes every snapshot observable, starting with the initial one.
    mutating func setup() async throws {
        await subscribe()
        sut.setup()
        await waitForPeriodicTasks()
    }

    /// Periodic tasks trigger before sleeping, so both pending sleeps put setup triggers ahead of the test's.
    func waitForPeriodicTasks() async {
        await clock.waitForSleeps(for: .seconds(60), count: 2)
    }

    func tick() async {
        await clock.resumeSleeps(for: .seconds(60), count: 2)
    }

    func throttleAndWaitForTasks() async {
        let tasks = observationTasks

        sut.throttle()

        for task in tasks {
            await task?.value
        }
    }

    mutating func teardown() {
        sut.throttle()
        iterator = nil
        statusStream = nil
    }

    mutating func nextStatuses() async throws -> Set<FiatOnrampTransactionStatusPayload> {
        try await iterator?.next() ?? []
    }

    mutating func assertNextStatuses(
        expected: Set<FiatOnrampTransactionStatusPayload>,
        after op: () async -> Void
    ) async throws {
        await op()
        let nextStatuses = try await nextStatuses()
        expectNoDifference(expected, nextStatuses)
    }
}

/// Buffers executions emitted before the auto-swap task subscribes, which a passthrough subject would drop.
final class TestDepositService: DepositServiceProtocol, @unchecked Sendable {
    private struct State {
        var pendingExecutions: [[DepositExecutionItem]] = []
        var channel: AsyncBufferedChannel<[DepositExecutionItem]>?
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func fetchDepositInfo(for _: ChainAssetId) async throws -> DepositServiceInfo {
        fatalError()
    }

    func executions() async -> AnyAsyncSequence<[DepositExecutionItem]> {
        let channel = AsyncBufferedChannel<[DepositExecutionItem]>()

        state.withLock { state in
            state.pendingExecutions.forEach(channel.send)
            state.pendingExecutions = []
            state.channel = channel
        }

        return channel.eraseToAnyAsyncSequence()
    }

    func setup() async {}

    func throttle() async {}

    func emitExecutions(_ executions: [DepositExecutionItem]) {
        state.withLock { state in
            if let channel = state.channel {
                channel.send(executions)
            } else {
                state.pendingExecutions.append(executions)
            }
        }
    }
}

actor TestFiatOnrampService: FiatOnrampServicing {
    private var stubbedWidgetResponse: FiatOnrampWidgetSessionResponse?
    private var stubbedTransactionSummaries: [FiatOnrampTransactionSummary] = []
    private var stubbedSessionTransactions: [FiatOnrampTransactionSummary] = []

    private var fetchTransactionsFailuresLeft: Int = 0
    private var fetchTransactionFailingIds: Set<FiatOnRampTransactionId> = []
    private var fetchTransactionRequests: [FiatOnRampTransactionId] = []
    private var fetchTransactionsRequests: [FiatOnrampTransactionQuery] = []

    func setStubbedWidgetResponse(_ response: FiatOnrampWidgetSessionResponse?) {
        stubbedWidgetResponse = response
    }

    func setStubbedTransactionSummaries(_ summaries: [FiatOnrampTransactionSummary]) {
        stubbedTransactionSummaries = summaries
    }

    func setStubbedSessionTransactions(_ summaries: [FiatOnrampTransactionSummary]) {
        stubbedSessionTransactions = summaries
    }

    func setFetchTransactionsFailuresLeft(_ failuresLeft: Int) {
        fetchTransactionsFailuresLeft = failuresLeft
    }

    func setFetchTransactionFailingIds(_ ids: Set<FiatOnRampTransactionId>) {
        fetchTransactionFailingIds = ids
    }

    func fetchTransactionRequestsSnapshot() -> [FiatOnRampTransactionId] {
        fetchTransactionRequests
    }

    func fetchTransactionsRequestsSnapshot() -> [FiatOnrampTransactionQuery] {
        fetchTransactionsRequests
    }

    func clearFetchTransactionRequests() {
        fetchTransactionRequests.removeAll()
    }

    func fetchFiatPurchaseLimits() async throws -> FiatOnrampFiatPurchaseLimitsResponse {
        fatalError()
    }

    func fetchCryptoQuote(_: FiatOnrampQuoteRequest) async throws -> [FiatOnrampQuoteSummary] {
        fatalError()
    }

    func createWidgetSession(_: FiatOnrampWidgetSessionRequest) async throws -> FiatOnrampWidgetSessionResponse {
        guard let stubbedWidgetResponse else {
            throw TestTrackingError.simulated
        }
        return stubbedWidgetResponse
    }

    func fetchServiceProviders() async throws -> [FiatOnrampProviderSummary] {
        fatalError()
    }

    func fetchTransaction(id: FiatOnRampTransactionId) async throws -> FiatOnrampTransactionSummary? {
        fetchTransactionRequests.append(id)
        if fetchTransactionFailingIds.contains(id) {
            throw TestTrackingError.simulated
        }
        return stubbedTransactionSummaries.first { $0.transactionId == id }
    }

    func fetchTransactions(_ request: FiatOnrampTransactionQuery) async throws -> [FiatOnrampTransactionSummary] {
        fetchTransactionsRequests.append(request)
        if fetchTransactionsFailuresLeft > 0 {
            fetchTransactionsFailuresLeft -= 1
            throw TestTrackingError.simulated
        }
        return stubbedSessionTransactions.filter { summary in
            request.sessionIds.contains(summary.sessionId)
        }
    }
}

actor TestFiatOnrampStorage: FiatOnrampStoring {
    private var sessionIds: Set<FiatOnrampPendingSession> = []
    private var trackedTransactions: Set<FiatOnrampTrackedTransaction> = []
    var addedTrackedTransactions: [Set<FiatOnrampTrackedTransaction>] = []

    func addSessionId(_ id: FiatOnRampSessionId) {
        addSessionId(id, createdAt: FiatOnrampTransactionTrackingTests.constantDate.timeIntervalSince1970)
    }

    func addSessionId(_ id: FiatOnRampSessionId, createdAt: TimeInterval) {
        if let existing = sessionIds.first(where: { $0.id == id }) {
            sessionIds.remove(existing)
            sessionIds.insert(
                .init(id: id, createdAt: min(existing.createdAt, createdAt))
            )
        } else {
            sessionIds.insert(.init(id: id, createdAt: createdAt))
        }
    }

    func removeSessionIds(_ ids: Set<FiatOnRampSessionId>) {
        sessionIds = sessionIds.filter { !ids.contains($0.id) }
    }

    func removeExpiredSessionIds(olderThan cutoff: TimeInterval) -> Set<FiatOnRampSessionId> {
        let expiredIds = Set(
            sessionIds.compactMap { session in
                session.createdAt <= cutoff ? session.id : nil
            }
        )
        sessionIds = sessionIds.filter { !expiredIds.contains($0.id) }
        return expiredIds
    }

    func getSessionIds() -> Set<FiatOnRampSessionId> {
        Set(sessionIds.map(\.id))
    }

    func addTrackedTransactions(_ transactions: Set<FiatOnrampTrackedTransaction>) {
        addedTrackedTransactions.append(transactions)

        var trackedTransactions = Array(trackedTransactions)

        for transaction in transactions {
            trackedTransactions.removeAll { $0.id == transaction.id }
        }

        trackedTransactions.append(contentsOf: transactions)
        self.trackedTransactions = Set(trackedTransactions)
    }

    func removeTrackedTransactions(_ ids: Set<FiatOnRampTransactionId>) {
        trackedTransactions = trackedTransactions.filter { transaction in
            !ids.contains(transaction.id)
        }
    }

    func getTrackedTransactions() -> Set<FiatOnrampTrackedTransaction> {
        trackedTransactions
    }
}
