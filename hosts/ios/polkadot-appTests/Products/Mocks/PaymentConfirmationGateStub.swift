import SubstrateSdk
@testable import polkadot_app

struct PaymentConfirmationGateStub: PaymentConfirmationGating {
    let requiresAuthentication: Bool

    func requiresAuthentication(amount _: Balance) async -> Bool {
        requiresAuthentication
    }
}
