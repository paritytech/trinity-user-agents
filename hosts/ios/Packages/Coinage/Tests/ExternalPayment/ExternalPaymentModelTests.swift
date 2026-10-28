import struct TrUAPIHost.ProductId
import Foundation
import Testing
@testable import Coinage

struct ExternalPaymentModelTests {
    @Test func identityIsScopedByProduct() {
        let one = ExternalPaymentTestFactory.payment(productId: try! ProductId("getcash.dot"), paymentId: "0xaa")
        let other = ExternalPaymentTestFactory.payment(productId: try! ProductId("other.dot"), paymentId: "0xaa")

        #expect(one.identifier == "external-payment:getcash.dot:0xaa")
        #expect(one.identifier != other.identifier)
        #expect(one.productId?.rawValue == "getcash.dot")
        #expect(one.paymentId == "0xaa")
    }

    @Test func walletPaymentKeepsItsStoredIdentityWithoutAProductOwner() {
        let payment = ExternalPaymentTestFactory.payment(productId: nil, paymentId: "0xaa")

        #expect(payment.productId == nil)
        #expect(payment.identifier == "external-payment:native-payment:0xaa")
    }

    @Test func terminalStages() {
        #expect(ExternalPayment.Stage.completed.isTerminal)
        #expect(ExternalPayment.Stage.failed.isTerminal)
        #expect(ExternalPayment.Stage.partiallyCompleted.isTerminal)
        #expect(!ExternalPayment.Stage.plan.isTerminal)
        #expect(!ExternalPayment.Stage.onboardCoins.isTerminal)
        #expect(!ExternalPayment.Stage.offboardVouchers.isTerminal)
    }

    @Test func nonTerminalStagesPrecedeTerminalOnes() {
        // The store's non-terminal query is `stage < completed`.
        let terminal = ExternalPayment.Stage.completed.rawValue
        #expect([ExternalPayment.Stage.plan, .onboardCoins, .offboardVouchers].allSatisfy { $0.rawValue < terminal })
        #expect([ExternalPayment.Stage.failed, .partiallyCompleted].allSatisfy { $0.rawValue >= terminal })
    }
}
