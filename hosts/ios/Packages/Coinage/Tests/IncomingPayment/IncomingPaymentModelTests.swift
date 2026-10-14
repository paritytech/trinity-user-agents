import Testing
import Foundation
import BigInt
import SubstrateSdk
@testable import Coinage

struct IncomingPaymentModelTests {
    @Test func groupIdIsProductBoundAndPrefixed() {
        #expect(IncomingPayment.groupId(productId: "prodA", paymentId: "pay1") == "top up:prodA:pay1")

        let payment = IncomingPayment(
            paymentId: "pay1",
            productId: "prodA",
            amount: 0,
            createdAt: Date(),
            outcome: nil
        )
        #expect(payment.groupId == "top up:prodA:pay1")
    }

    @Test func isActiveReflectsOutcome() {
        func make(_ outcome: IncomingPaymentTerminalOutcome?) -> IncomingPayment {
            IncomingPayment(paymentId: "p", productId: "prod", amount: 0, createdAt: Date(), outcome: outcome)
        }
        #expect(make(nil).isActive)
        #expect(!make(.claimed).isActive)
        #expect(!make(.notClaimed).isActive)
        #expect(!make(.claimedPartially(actualClaimed: 5)).isActive)
    }

    @Test func onlyFinalizedClaimedAndPartialAndNotClaimedAreTerminal() {
        #expect(!IncomingPaymentStatus.detecting.isTerminal)
        #expect(!IncomingPaymentStatus.claiming.isTerminal)
        #expect(!IncomingPaymentStatus.claimed(finalized: false).isTerminal)
        #expect(IncomingPaymentStatus.claimed(finalized: true).isTerminal)
        #expect(IncomingPaymentStatus.claimedPartially(actualClaimed: 5).isTerminal)
        #expect(IncomingPaymentStatus.notClaimed.isTerminal)
    }

    @Test func statusMapsFromDetection() {
        #expect(IncomingPaymentStatus(detection: .detecting) == .detecting)
        #expect(IncomingPaymentStatus(detection: .claiming) == .claiming)
        #expect(IncomingPaymentStatus(detection: .claimingRest(claimed: 3)) == .claiming)
        #expect(IncomingPaymentStatus(detection: .claimed(amount: 10, finalized: true)) == .claimed(finalized: true))
        #expect(IncomingPaymentStatus(detection: .claimedPartially(claimed: 4)) == .claimedPartially(actualClaimed: 4))
        #expect(IncomingPaymentStatus(detection: .notClaimed) == .notClaimed)
    }

    @Test func statusMapsFromStoredOutcome() {
        #expect(IncomingPaymentStatus(outcome: .claimed) == .claimed(finalized: true))
        #expect(IncomingPaymentStatus(outcome: .claimedPartially(actualClaimed: 7)) ==
            .claimedPartially(actualClaimed: 7))
        #expect(IncomingPaymentStatus(outcome: .notClaimed) == .notClaimed)
    }

    @Test func onlyTerminalStatusesYieldAVerdict() {
        #expect(IncomingPaymentStatus.claimed(finalized: true).terminalOutcome == .claimed)
        #expect(IncomingPaymentStatus.claimed(finalized: false).terminalOutcome == nil)
        #expect(IncomingPaymentStatus.claimedPartially(actualClaimed: 3)
            .terminalOutcome == .claimedPartially(actualClaimed: 3))
        #expect(IncomingPaymentStatus.notClaimed.terminalOutcome == .notClaimed)
        #expect(IncomingPaymentStatus.detecting.terminalOutcome == nil)
        #expect(IncomingPaymentStatus.claiming.terminalOutcome == nil)
    }
}

struct IncomingPaymentSourceDescriptorTests {
    @Test func productAccountMatchesOnTheFullPath() {
        // The path embeds the product, so the same index under another product is another path.
        let a = IncomingPaymentSourceDescriptor.productAccount(derivationPath: "//product//a/0x1")
        #expect(a.drawsOnSameFunds(as: .productAccount(derivationPath: "//product//a/0x1")))
        #expect(!a.drawsOnSameFunds(as: .productAccount(derivationPath: "//product//b/0x1")))
        #expect(!a.drawsOnSameFunds(as: .productAccount(derivationPath: "//product//a/0x9")))
    }

    @Test func privateKeyMatchesItself() {
        let a = IncomingPaymentSourceDescriptor.privateKey(secretKey: Data([1]))
        #expect(a.drawsOnSameFunds(as: .privateKey(secretKey: Data([1]))))
        #expect(!a.drawsOnSameFunds(as: .privateKey(secretKey: Data([2]))))
    }

    @Test func coinsMatchOnAnyOverlap() {
        let a = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([1]), Data([2])])
        #expect(a.drawsOnSameFunds(as: .coins(secretKeys: [Data([2])])))
        #expect(!a.drawsOnSameFunds(as: .coins(secretKeys: [Data([3])])))
    }

    @Test func differentShapesNeverCollide() {
        let a = IncomingPaymentSourceDescriptor.privateKey(secretKey: Data([1]))
        #expect(!a.drawsOnSameFunds(as: .coins(secretKeys: [Data([1])])))
    }
}
