import Foundation
import Testing
import SubstrateSdk
@testable import Products

@Suite("AutoAllowPaymentApprovalRequester Tests")
struct AutoAllowPaymentApprovalRequesterTests {
    private let allowedLabels = Set(["getcash"])
    private let destination = Data(repeating: 7, count: 32)

    private func makeSUT(
        wrappedDecision: PaymentApprovalDecision = .rejected
    ) -> (requester: AutoAllowPaymentApprovalRequester, wrapped: MockPaymentApprovalRequester) {
        let wrapped = MockPaymentApprovalRequester()
        wrapped.decision = wrappedDecision
        let requester = AutoAllowPaymentApprovalRequester(allowedLabels: allowedLabels, wrapped: wrapped)
        return (requester, wrapped)
    }

    @Test("approves allowlisted product without asking")
    func approvesAllowlisted() async {
        let (requester, wrapped) = makeSUT()

        let decision = await requester.requestApproval(
            productId: "getcash.dot",
            amount: 100,
            destination: destination
        )

        #expect(decision == .approved)
        #expect(wrapped.calls.isEmpty)
    }

    @Test("delegates non-allowlisted product to wrapped")
    func delegatesNonAllowlisted() async {
        let (requester, wrapped) = makeSUT(wrappedDecision: .rejected)

        let decision = await requester.requestApproval(
            productId: "other.dot",
            amount: 100,
            destination: destination
        )

        #expect(decision == .rejected)
        #expect(wrapped.calls.count == 1)
        #expect(wrapped.calls.first?.productId == "other.dot")
    }

    @Test("delegates bare productId without root", arguments: ["getcash", "getcash..dot"])
    func delegatesMalformedProductId(productId: String) async {
        let (requester, wrapped) = makeSUT(wrappedDecision: .approved)

        let decision = await requester.requestApproval(
            productId: productId,
            amount: 100,
            destination: destination
        )

        #expect(decision == .approved)
        #expect(wrapped.calls.count == 1)
    }
}
