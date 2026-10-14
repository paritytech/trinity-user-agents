import Foundation
import Testing
import Coinage
@testable import polkadot_app

@Suite("ExternalPaymentMapper")
struct ExternalPaymentMapperTests {
    private let destination = Data(repeating: 9, count: 32)

    private func makeStore() -> ExternalPaymentCoreDataStore {
        ExternalPaymentCoreDataStore(storageFacade: UserDataStorageTestFacade())
    }

    @Test("round-trips the product-scoped identity, settled value and planned vouchers")
    func roundTripsIdentityAndStageData() async throws {
        let store = makeStore()
        var payment = ExternalPayment(
            productId: "getcash.dot",
            paymentId: "0xab",
            amountInPlanks: 1_500,
            destination: destination
        )
        try await store.save(payment: payment)

        payment.stage = .offboardVouchers
        payment.plannedVoucherIndices = [7, 42]
        payment.surplusInPlanks = 250
        try await store.save(payment: payment)

        let fetched = try #require(try await store.fetchPayment(byId: payment.identifier))
        #expect(fetched.identifier == "external-payment:getcash.dot:0xab")
        #expect(fetched.productId == "getcash.dot")
        #expect(fetched.paymentId == "0xab")
        #expect(fetched.amountInPlanks == 1_500)
        #expect(fetched.stage == .offboardVouchers)
        #expect(fetched.plannedVoucherIndices == [7, 42])
        #expect(fetched.surplusInPlanks == 250)
        #expect(fetched.settledInPlanks == 0)
    }

    @Test("a stage update persists the settled value and clears the planned vouchers")
    func stageUpdatePersistsSettledValue() async throws {
        let store = makeStore()
        var payment = ExternalPayment(
            productId: "getcash.dot",
            paymentId: "0xcd",
            amountInPlanks: 5,
            destination: destination,
            stage: .offboardVouchers,
            plannedVoucherIndices: [1]
        )
        try await store.save(payment: payment)

        payment.stage = .partiallyCompleted
        payment.settledInPlanks = 3
        payment.plannedVoucherIndices = []
        try await store.save(payment: payment)

        let fetched = try #require(try await store.fetchPayment(byId: payment.identifier))
        #expect(fetched.stage == .partiallyCompleted)
        #expect(fetched.settledInPlanks == 3)
        #expect(fetched.plannedVoucherIndices.isEmpty)
    }
}
