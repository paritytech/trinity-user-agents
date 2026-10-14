import Foundation
import Testing
@testable import Coinage

/// Durability scenario: a row persisted mid-flight by a "previous run" reaches a terminal stage in
/// a fresh service over the same store, without recycling or registering anything twice.
struct ExternalPaymentRestartTests {
    private typealias Factory = ExternalPaymentTestFactory

    private let voucher = Factory.voucher(index: 1)

    @Test func resumesFromPlanAndRegistersOnce() async throws {
        let payment = Factory.payment(stage: .plan)
        let store = InMemoryExternalPaymentStore(seed: [payment])
        let harness = Factory.makeHarness(store: store, vouchers: [voucher])
        harness.planner.setDefault(.success(Factory.unloadPreview([voucher])))

        harness.service.setup(with: Factory.denomination)

        _ = try await store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(harness.txService.registrations == [Factory.unloadGroupId(for: payment)])
        #expect(harness.recycler.submissions.isEmpty)
    }

    @Test func resumesOnboardingByRejoiningTheRecyclingGroup() async throws {
        let recycled = Factory.voucher(index: 2)
        let payment = Factory.payment(
            amount: Factory.planks(3) + Factory.planks(3),
            stage: .onboardCoins,
            plannedVoucherIndices: [1]
        )
        let store = InMemoryExternalPaymentStore(seed: [payment])
        let harness = Factory.makeHarness(store: store, vouchers: [voucher, recycled])
        let groupId = Factory.recycleGroupId(for: payment)
        harness.recycler.markExisting(groupId)
        harness.recycler.script(
            groupId: groupId,
            statuses: [.pending, .allRecycled(vouchers: [Factory.tracked(recycled)], finalized: false)]
        )
        harness.txService.seedGroup(groupId, statuses: [.pendingSuccess])

        harness.service.setup(with: Factory.denomination)

        _ = try await store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(harness.recycler.submissions.isEmpty)
        #expect(harness.planner.calls.isEmpty)
        #expect(harness.txService.registrations == [Factory.unloadGroupId(for: payment)])
    }

    @Test func resumesOffboardingByRejoiningTheUnloadGroup() async throws {
        let payment = Factory.payment(stage: .offboardVouchers, plannedVoucherIndices: [1])
        let store = InMemoryExternalPaymentStore(seed: [payment])
        let harness = Factory.makeHarness(store: store, vouchers: [voucher])
        harness.txService.seedGroup(Factory.unloadGroupId(for: payment), statuses: [.finalizedSuccess])

        harness.service.setup(with: Factory.denomination)

        _ = try await store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(harness.txService.registrations.isEmpty)
        #expect(harness.planner.calls.isEmpty)
    }

    @Test func resumesOffboardingWithThePersistedVouchersWhenNothingWasRegistered() async throws {
        let payment = Factory.payment(stage: .offboardVouchers, plannedVoucherIndices: [1])
        let store = InMemoryExternalPaymentStore(seed: [payment])
        let harness = Factory.makeHarness(store: store, vouchers: [voucher])

        harness.service.setup(with: Factory.denomination)

        _ = try await store.awaitPayment(id: payment.identifier) { $0.stage == .completed }
        #expect(harness.txService.registrations == [Factory.unloadGroupId(for: payment)])
        #expect(harness.planner.calls.isEmpty)
    }
}
