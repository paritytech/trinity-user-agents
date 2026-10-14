import Foundation
import Testing
@testable import Coinage

struct ExternalPaymentModelTests {
    @Test func identityIsScopedByProduct() {
        let one = ExternalPaymentTestFactory.payment(productId: "getcash.dot", paymentId: "0xaa")
        let other = ExternalPaymentTestFactory.payment(productId: "other.dot", paymentId: "0xaa")

        #expect(one.identifier == "external-payment:getcash.dot:0xaa")
        #expect(one.identifier != other.identifier)
        #expect(one.productId == "getcash.dot")
        #expect(one.paymentId == "0xaa")
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
