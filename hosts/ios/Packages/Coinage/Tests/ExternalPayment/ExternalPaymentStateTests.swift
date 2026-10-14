import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

struct ExternalPaymentStateTests {
    private typealias Factory = ExternalPaymentTestFactory

    private let voucher = Factory.voucher(index: 1)
    private let coin = Factory.coin(index: 9)

    // MARK: - Plan

    @Test func unloadPlanMovesToOffboarding() async {
        let planner = StubExternalPaymentPlanner(defaultResult: .success(Factory.unloadPreview([voucher])))
        let factory = Factory.makeStateFactory(planner: planner)

        let next = await PlanPaymentState(payment: Factory.payment()).transit(with: factory)
        let memo = await next.memo()

        #expect(memo.stage == .offboardVouchers)
        #expect(memo.plannedVoucherIndices == [1])
    }

    @Test func loadCoinsPlanMovesToOnboardingWithExactVouchers() async {
        let preview = Factory.loadCoinsPreview(coins: [coin], exactVouchers: [voucher])
        let planner = StubExternalPaymentPlanner(defaultResult: .success(preview))
        let recycler = StubCoinageRecyclingService()
        let payment = Factory.payment(amount: Factory.planks(4))
        let factory = Factory.makeStateFactory(planner: planner, recycler: recycler)

        let next = await PlanPaymentState(payment: payment).transit(with: factory)
        let memo = await next.memo()
        #expect(memo.stage == .onboardCoins)
        #expect(memo.plannedVoucherIndices == [1])

        // Driving the onboarding state proves it carries the coins of the plan.
        _ = await next.transit(with: factory)
        #expect(recycler.submissions == [.init(coins: [coin], groupId: Factory.recycleGroupId(for: payment))])
    }

    @Test func planNotEnoughBalanceIsAVerdict() async {
        let factory = Factory
            .makeStateFactory(planner: StubExternalPaymentPlanner(defaultResult: .success(.notEnoughBalance)))

        let memo = await PlanPaymentState(payment: Factory.payment()).transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "insufficient balance")
    }

    @Test func planThrowFailsWithoutRetry() async {
        let planner = StubExternalPaymentPlanner(defaultResult: .failure(StubExternalPaymentPlanner.Failure("rpc")))
        let factory = Factory.makeStateFactory(planner: planner)

        let next = await PlanPaymentState(payment: Factory.payment()).transit(with: factory)
        let memo = await next.memo()

        #expect(next.isTerminal)
        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "rpc")
    }

    // MARK: - Onboard coins

    @Test func onboardWaitsForRecyclingThenOffboardsExactPlusRecycled() async {
        let exact = Factory.voucher(index: 1, exponent: 2)
        let recycled = Factory.voucher(index: 2, exponent: 3)
        let payment = Factory.payment(amount: Factory.planks(3) + Factory.planks(2))
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.script(
            groupId: groupId,
            statuses: [.pending, .allRecycled(vouchers: [Factory.tracked(recycled)], finalized: false)]
        )
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.pendingSuccess])
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            recycler: recycler,
            txService: txService,
            vouchers: [exact, recycled]
        )

        let next = await OnboardCoinsPaymentState(payment: payment, coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory)
        let memo = await next.memo()

        #expect(memo.stage == .offboardVouchers)
        #expect(memo.plannedVoucherIndices == [2, 1])
        #expect(recycler.submissions == [.init(coins: [coin], groupId: groupId)])
        _ = await next.transit(with: factory)
        #expect(txService.registrations == [Factory.unloadGroupId(for: payment)])
    }

    @Test func onboardUnloadsOnlyWhatTheAmountNeedsAfterRecycling() async {
        // Exact 1000 + recycled 1000 for a payment of 1000: one voucher is enough.
        let exact = Factory.voucher(index: 1, exponent: 3)
        let recycled = Factory.voucher(index: 2, exponent: 3)
        let payment = Factory.payment(amount: Factory.planks(3))
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.script(
            groupId: groupId,
            statuses: [.allRecycled(vouchers: [Factory.tracked(recycled)], finalized: true)]
        )
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.finalizedSuccess])
        let planner = StubExternalPaymentPlanner()
        let factory = Factory.makeStateFactory(
            planner: planner, recycler: recycler, txService: txService, vouchers: [exact, recycled]
        )

        let memo = await OnboardCoinsPaymentState(payment: payment, coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .offboardVouchers)
        #expect(memo.plannedVoucherIndices.count == 1)
        #expect(planner.pickCalls.count == 1)
        #expect(Set(planner.pickCalls[0].available) == [1, 2])
    }

    @Test func onboardFailsWhenRecycledVouchersStillFallShort() async {
        let payment = Factory.payment(amount: Factory.planks(5))
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.script(
            groupId: groupId,
            statuses: [.allRecycled(
                vouchers: [Factory.tracked(Factory.voucher(index: 2, exponent: 2))],
                finalized: true
            )]
        )
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.finalizedSuccess])
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(), recycler: recycler, txService: txService, vouchers: [voucher]
        )

        let memo = await OnboardCoinsPaymentState(payment: payment, coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "insufficient balance after recycling")
    }

    @Test func onboardFailsOnIncompleteRecyclingWithoutRetry() async {
        let payment = Factory.payment()
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.script(groupId: groupId, statuses: [.pending, .incomplete])
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.failure])
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            recycler: recycler,
            txService: txService
        )

        let memo = await OnboardCoinsPaymentState(payment: payment, coins: [coin], exactVoucherIndices: [])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "recycling incomplete")
        #expect(recycler.submissions.count == 1)
    }

    @Test func onboardFailsWhenTheStreamEndsWithoutAVerdict() async {
        let payment = Factory.payment()
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.script(groupId: groupId, statuses: [.pending])
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.pending])
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            recycler: recycler,
            txService: txService
        )

        let memo = await OnboardCoinsPaymentState(payment: payment, coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
    }

    @Test func onboardSubmissionErrorFails() async {
        let recycler = StubCoinageRecyclingService()
        recycler.setError(StubExternalPaymentPlanner.Failure("recycle"))
        let factory = Factory.makeStateFactory(planner: StubExternalPaymentPlanner(), recycler: recycler)

        let memo = await OnboardCoinsPaymentState(payment: Factory.payment(), coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "recycle")
    }

    @Test func onboardFailsWhenNothingWasRegistered() async {
        // Every coin's preparation failed: the recycler submitted nothing and no group exists.
        let factory = Factory.makeStateFactory(planner: StubExternalPaymentPlanner())

        let memo = await OnboardCoinsPaymentState(payment: Factory.payment(), coins: [coin], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "recycling submission failed")
    }

    @Test func onboardReentryRejoinsTheGroupAndOffboardsThePersistedSelection() async {
        let recycled = Factory.voucher(index: 2)
        let payment = Factory.payment(amount: Factory.planks(3) + Factory.planks(3))
        let recycler = StubCoinageRecyclingService()
        let groupId = Factory.recycleGroupId(for: payment)
        recycler.markExisting(groupId)
        recycler.script(
            groupId: groupId,
            statuses: [.allRecycled(vouchers: [Factory.tracked(recycled)], finalized: true)]
        )
        let txService = StubGroupTxService()
        txService.seedGroup(groupId, statuses: [.finalizedSuccess])
        let planner = StubExternalPaymentPlanner()
        let factory = Factory.makeStateFactory(
            planner: planner, recycler: recycler, txService: txService, vouchers: [voucher, recycled]
        )

        let memo = await OnboardCoinsPaymentState(payment: payment, coins: [], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .offboardVouchers)
        #expect(Set(memo.plannedVoucherIndices) == [1, 2])
        #expect(recycler.submissions.isEmpty)
        #expect(planner.calls.isEmpty)
    }

    @Test func onboardReentryWithoutARegisteredGroupFailsInsteadOfReplanning() async {
        let planner = StubExternalPaymentPlanner()
        let factory = Factory.makeStateFactory(planner: planner)

        let memo = await OnboardCoinsPaymentState(payment: Factory.payment(), coins: [], exactVoucherIndices: [1])
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "recycling group not found")
        #expect(planner.calls.isEmpty)
    }

    // MARK: - Offboard vouchers

    @Test func offboardSuccessCompletesWithFullSettlement() async {
        let txService = StubGroupTxService()
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            txService: txService,
            vouchers: [voucher]
        )
        let payment = Factory.payment()

        let memo = await OffboardVouchersPaymentState(payment: payment, voucherIndices: [1], surplus: 0)
            .transit(with: factory)
            .memo()

        #expect(memo.stage == .completed)
        #expect(memo.settledInPlanks == payment.amountInPlanks)
        #expect(txService.registrations == [Factory.unloadGroupId(for: payment)])
    }

    @Test func offboardPartialOutcomeIsTerminalWithTheSettledValue() async {
        let txService = StubGroupTxService()
        txService.setOutcome(.partial)
        let vouchers = [Factory.voucher(index: 1, exponent: 3), Factory.voucher(index: 2, exponent: 2)]
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            txService: txService,
            vouchers: vouchers
        )
        let payment = Factory.payment(amount: Factory.planks(3) + Factory.planks(2))

        let next = await OffboardVouchersPaymentState(payment: payment, voucherIndices: [1, 2], surplus: 0)
            .transit(with: factory)
        let memo = await next.memo()

        #expect(next.isTerminal)
        #expect(memo.stage == .partiallyCompleted)
        #expect([Factory.planks(3), Factory.planks(2)].contains(memo.settledInPlanks))
        #expect(memo.failureReason == "1 of 2 unload transactions executed")
    }

    @Test func offboardFailedOutcomeIsAVerdict() async {
        let txService = StubGroupTxService()
        txService.setOutcome(.failure)
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            txService: txService,
            vouchers: [voucher]
        )

        let memo = await OffboardVouchersPaymentState(payment: Factory.payment(), voucherIndices: [1], surplus: 0)
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "no unload transaction executed")
    }

    @Test func offboardSubmissionErrorFailsWithoutRetry() async {
        let txService = StubGroupTxService()
        txService.setSubmitError(StubExternalPaymentPlanner.Failure("submit"))
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            txService: txService,
            vouchers: [voucher]
        )

        let memo = await OffboardVouchersPaymentState(payment: Factory.payment(), voucherIndices: [1], surplus: 0)
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(memo.failureReason == "submit")
    }

    @Test func offboardWithUnknownVouchersFails() async {
        let txService = StubGroupTxService()
        let factory = Factory.makeStateFactory(planner: StubExternalPaymentPlanner(), txService: txService)

        let memo = await OffboardVouchersPaymentState(payment: Factory.payment(), voucherIndices: [1], surplus: 0)
            .transit(with: factory).memo()

        #expect(memo.stage == .failed)
        #expect(txService.registrations.isEmpty)
    }

    @Test func offboardRejoinsARegisteredGroupWithoutRegisteringAgain() async {
        let txService = StubGroupTxService()
        let payment = Factory.payment()
        txService.seedGroup(Factory.unloadGroupId(for: payment), statuses: [.finalizedSuccess])
        let factory = Factory.makeStateFactory(
            planner: StubExternalPaymentPlanner(),
            txService: txService,
            vouchers: [voucher]
        )

        let memo = await OffboardVouchersPaymentState(payment: payment, voucherIndices: [1], surplus: 0)
            .transit(with: factory)
            .memo()

        #expect(memo.stage == .completed)
        #expect(txService.registrations.isEmpty)
    }

    @Test func memoRestoreCarriesTheStageAndItsVouchers() async {
        let factory = Factory.makeStateFactory(planner: StubExternalPaymentPlanner())

        let onboard = await factory.stateFromMemo(payment: Factory.payment(
            stage: .onboardCoins,
            plannedVoucherIndices: [3]
        )).memo()
        let offboard = await factory.stateFromMemo(payment: Factory.payment(
            stage: .offboardVouchers,
            plannedVoucherIndices: [4]
        )).memo()

        #expect(onboard.stage == .onboardCoins)
        #expect(onboard.plannedVoucherIndices == [3])
        #expect(offboard.stage == .offboardVouchers)
        #expect(offboard.plannedVoucherIndices == [4])
        #expect(await factory.stateFromMemo(payment: Factory.payment(stage: .plan)).memo().stage == .plan)
    }
}
