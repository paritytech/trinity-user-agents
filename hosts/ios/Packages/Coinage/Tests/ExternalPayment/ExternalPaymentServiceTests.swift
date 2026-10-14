import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

struct ExternalPaymentServiceTests {
    private typealias Factory = ExternalPaymentTestFactory

    private let voucher = Factory.voucher(index: 1)

    // MARK: - Registration

    @Test func initiatePersistsAPlanRowUnderTheProductScopedId() async throws {
        let harness = Factory.makeHarness()

        try await harness.service.initiatePayment(
            productId: "getcash.dot",
            paymentId: "0xaa",
            amountInPlanks: Factory.planks(3),
            destination: Factory.destination
        )

        let stored = try #require(harness.store.payment(id: "external-payment:getcash.dot:0xaa"))
        #expect(stored.stage == .plan)
        #expect(stored.productId == "getcash.dot")
        #expect(stored.paymentId == "0xaa")
        #expect(stored.amountInPlanks == Factory.planks(3))
    }

    @Test func replayIsRefused() async throws {
        let harness = Factory.makeHarness()
        try await harness.service.initiatePayment(
            productId: "getcash.dot", paymentId: "0xaa", amountInPlanks: 1, destination: Factory.destination
        )

        await #expect(throws: ExternalPaymentError.alreadyExists) {
            try await harness.service.initiatePayment(
                productId: "getcash.dot", paymentId: "0xaa", amountInPlanks: 2, destination: Factory.destination
            )
        }
        #expect(harness.store.all().count == 1)
    }

    @Test func sameIdUnderAnotherProductIsAnotherPayment() async throws {
        let harness = Factory.makeHarness()
        try await harness.service.initiatePayment(
            productId: "getcash.dot", paymentId: "0xaa", amountInPlanks: 1, destination: Factory.destination
        )
        try await harness.service.initiatePayment(
            productId: "other.dot", paymentId: "0xaa", amountInPlanks: 1, destination: Factory.destination
        )

        #expect(harness.store.all().count == 2)
    }

    @Test func emptyIdIsRefused() async {
        let harness = Factory.makeHarness()

        await #expect(throws: ExternalPaymentError.invalidPaymentId) {
            try await harness.service.initiatePayment(
                productId: "getcash.dot", paymentId: "", amountInPlanks: 1, destination: Factory.destination
            )
        }
    }

    @Test func concurrentInitiationsOfOneIdentityRegisterExactlyOnce() async throws {
        let harness = Factory.makeHarness()

        let outcomes = await withTaskGroup(of: Bool.self, returning: [Bool].self) { group in
            for _ in 0 ..< 8 {
                group.addTask {
                    do {
                        try await harness.service.initiatePayment(
                            productId: "getcash.dot", paymentId: "0xaa", amountInPlanks: 1,
                            destination: Factory.destination
                        )
                        return true
                    } catch {
                        return false
                    }
                }
            }
            return await group.reduce(into: []) { $0.append($1) }
        }

        #expect(outcomes.filter { $0 }.count == 1)
        #expect(harness.store.all().count == 1)
    }

    // MARK: - Preview

    @Test func previewAndPrivacyCheckProxyToThePlanner() async throws {
        let harness = Factory.makeHarness()
        harness.planner.setDefault(.success(Factory.unloadPreview([voucher])))
        harness.planner.setPrivateAnswer(.success(false))

        let preview = try await harness.service.previewPayment(for: Factory.planks(3), context: Factory.denomination)
        let isPrivate = try await harness.service.canExecuteExternalPaymentPrivately(
            amount: Factory.planks(2),
            context: Factory.denomination
        )

        #expect(preview == Factory.unloadPreview([voucher]))
        #expect(!isPrivate)
        #expect(harness.planner.calls == [Factory.planks(3)])
        #expect(harness.planner.privateCalls == [Factory.planks(2)])
    }

    // MARK: - Status

    @Test func unknownIdFailsTheStreamWithNotFound() async {
        let harness = Factory.makeHarness()

        await #expect(throws: ExternalPaymentError.notFound) {
            for try await _ in harness.service.subscribePaymentStatus(productId: "getcash.dot", paymentId: "0xzz") {}
        }
    }

    @Test(arguments: [
        (ExternalPayment.Stage.completed, ExternalPaymentStatus.completed),
        (.failed, .failed(reason: "boom")),
        (.partiallyCompleted, .partiallyCompleted(settledInPlanks: 5))
    ])
    func coldSubscriptionReplaysTheTerminalStatusOnceAndEnds(
        stage: ExternalPayment.Stage,
        expected: ExternalPaymentStatus
    ) async throws {
        var payment = Factory.payment(settled: 5, stage: stage)
        payment.failureReason = "boom"
        let harness = Factory.makeHarness(store: InMemoryExternalPaymentStore(seed: [payment]))

        var received: [ExternalPaymentStatus] = []
        for try await status in harness.service.subscribePaymentStatus(
            productId: payment.productId,
            paymentId: payment.paymentId
        ) {
            received.append(status)
        }

        #expect(received == [expected])
    }

    @Test func duplicateSnapshotsCollapse() async throws {
        let payment = Factory.payment()
        let store = InMemoryExternalPaymentStore(seed: [payment])
        let harness = Factory.makeHarness(store: store)

        let collector = Task<[ExternalPaymentStatus], Error> {
            var received: [ExternalPaymentStatus] = []
            for try await status in harness.service.subscribePaymentStatus(
                productId: payment.productId,
                paymentId: payment.paymentId
            ) {
                received.append(status)
            }
            return received
        }

        try await Task.sleep(for: .milliseconds(50))
        var moved = payment
        moved.stage = .offboardVouchers
        try await store.save(payment: moved)
        moved.stage = .completed
        try await store.save(payment: moved)

        #expect(try await collector.value == [.processing, .completed])
    }

    // MARK: - Processing

    @Test func privatePlanCompletesInOnePass() async throws {
        let payment = Factory.payment()
        let harness = Factory.makeHarness(store: InMemoryExternalPaymentStore(seed: [payment]), vouchers: [voucher])
        harness.planner.setDefault(.success(Factory.unloadPreview([voucher])))

        harness.service.setup(with: Factory.denomination)

        let completed = try await harness.store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(completed.settledInPlanks == payment.amountInPlanks)
        #expect(harness.planner.calls.count == 1)
        #expect(harness.txService.registrations == [Factory.unloadGroupId(for: payment)])
    }

    @Test func aRecyclerOverTheConsolidationLimitIsOffboardedInSeveralCalls() async throws {
        // Five vouchers of one denomination all sit in the same recycler, so without chunking they
        // would be offboarded by a single call carrying more aliases than the pallet accepts.
        let vouchers = (1 ... 5).map { Factory.voucher(index: .harness(UInt64($0)), exponent: 3) }
        let payment = Factory.payment(amount: Factory.planks(3) * 5)
        let harness = Factory.makeHarness(
            store: InMemoryExternalPaymentStore(seed: [payment]),
            vouchers: vouchers,
            maxConsolidation: 2
        )
        harness.planner.setDefault(.success(Factory.unloadPreview(vouchers)))

        harness.service.setup(with: Factory.denomination)

        let completed = try await harness.store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(completed.settledInPlanks == payment.amountInPlanks)

        let entries = try await harness.txService
            .getOperationGroupStatuses(Factory.unloadGroupId(for: payment))
        #expect(entries.count == 3)
        #expect(entries.allSatisfy { $0.inputs.count <= 2 })
        #expect(entries.flatMap(\.inputs).count == 5)
    }

    @Test func surplusIsCarriedByExactlyOneCallWhenARecyclerIsSplit() async throws {
        // All five vouchers share a recycler, so every call carries the same recycler key: the
        // surplus has to be pinned to one call rather than to the key it was planned against.
        let vouchers = (1 ... 5).map { Factory.voucher(index: .harness(UInt64($0)), exponent: 3) }
        let surplus = Factory.planks(2)
        let payment = Factory.payment(amount: Factory.planks(3) * 5 - surplus)
        let harness = Factory.makeHarness(
            store: InMemoryExternalPaymentStore(seed: [payment]),
            vouchers: vouchers,
            maxConsolidation: 2
        )
        harness.planner.setDefault(.success(Factory.unloadPreview(vouchers, surplus: surplus)))

        harness.service.setup(with: Factory.denomination)

        _ = try await harness.store.awaitPayment(id: payment.identifier) { $0.stage == .completed }

        let entries = try await harness.txService
            .getOperationGroupStatuses(Factory.unloadGroupId(for: payment))
        #expect(entries.count == 3)
        #expect(entries.filter { !$0.outputs.isEmpty }.count == 1)
        #expect(entries.flatMap(\.outputs).count == 1)
    }

    @Test func plannerErrorFailsWithoutRetry() async throws {
        let payment = Factory.payment()
        let harness = Factory.makeHarness(store: InMemoryExternalPaymentStore(seed: [payment]))
        harness.planner.setDefault(.failure(StubExternalPaymentPlanner.Failure("rpc down")))

        harness.service.setup(with: Factory.denomination)

        let failed = try await harness.store.awaitPayment(id: payment.identifier) { $0.stage == .failed }
        try await Task.sleep(for: .milliseconds(100))
        #expect(harness.planner.calls.count == 1)
        #expect(failed.failureReason == "rpc down")
    }

    @Test func partialUnloadIsTerminalAndReportsTheSettledValue() async throws {
        let vouchers = [Factory.voucher(index: 1, exponent: 3), Factory.voucher(index: 2, exponent: 2)]
        let payment = Factory.payment(amount: Factory.planks(3) + Factory.planks(2))
        let harness = Factory.makeHarness(store: InMemoryExternalPaymentStore(seed: [payment]), vouchers: vouchers)
        harness.planner.setDefault(.success(Factory.unloadPreview(vouchers)))
        harness.txService.setOutcome(.partial)

        harness.service.setup(with: Factory.denomination)

        let partial = try await harness.store.awaitPayment(id: payment.identifier) { $0.stage == .partiallyCompleted }
        let settled = partial.settledInPlanks
        #expect([Factory.planks(3), Factory.planks(2)].contains(settled))
        #expect(harness.planner.calls.count == 1)

        var received: [ExternalPaymentStatus] = []
        for try await status in harness.service.subscribePaymentStatus(
            productId: payment.productId,
            paymentId: payment.paymentId
        ) {
            received.append(status)
        }
        #expect(received == [.partiallyCompleted(settledInPlanks: settled)])
    }

    @Test func paymentsAreProcessedInCreationOrder() async throws {
        let first = Factory.payment(paymentId: "0x01", createdAt: Date(timeIntervalSince1970: 1))
        let second = Factory.payment(paymentId: "0x02", createdAt: Date(timeIntervalSince1970: 2))
        let harness = Factory.makeHarness(store: InMemoryExternalPaymentStore(seed: [second, first]))
        harness.planner.setDefault(.success(.notEnoughBalance))

        harness.service.setup(with: Factory.denomination)

        _ = try await harness.store.awaitPayments { $0.allSatisfy { $0.stage == .failed } }
        #expect(harness.planner.calls.count == 2)
    }
}
