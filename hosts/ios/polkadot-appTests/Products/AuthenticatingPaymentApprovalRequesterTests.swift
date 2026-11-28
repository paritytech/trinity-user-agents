import Foundation
import Testing
import Products
import TrUAPIHost
@testable import polkadot_app

@Suite("AuthenticatingPaymentApprovalRequester")
struct AuthenticatingPaymentApprovalRequesterTests {
    @Test(
        "Asks for device authentication only for an approved payment above the limit",
        arguments: [
            (
                wrapped: PaymentApprovalDecision.approved,
                gate: false,
                authorized: false,
                expected: PaymentApprovalDecision.approved
            ),
            (wrapped: .approved, gate: true, authorized: true, expected: .approved),
            (wrapped: .approved, gate: true, authorized: false, expected: .rejected),
            (wrapped: .rejected, gate: true, authorized: true, expected: .rejected)
        ]
    )
    func decision(
        wrapped: PaymentApprovalDecision,
        gate: Bool,
        authorized: Bool,
        expected: PaymentApprovalDecision
    ) async throws {
        let requester = AuthenticatingPaymentApprovalRequester(
            gate: PaymentConfirmationGateStub(requiresAuthentication: gate),
            authorizeDeviceOwner: { authorized },
            wrapped: PaymentApprovalRequesterStub(decision: wrapped)
        )

        let decision = try await requester.requestApproval(
            productId: ProductId("shop.dot"),
            amount: 100,
            destination: Data(repeating: 7, count: 32)
        )

        #expect(decision == expected)
    }
}
