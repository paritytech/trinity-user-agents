import TrUAPIHost

extension FundingFlowModel {
    enum Screen: Hashable {
        case summary
        case fees
        case country
        case providers
        case network
        case token
        case deposit
        case progress
        case cancelConfirm
    }

    struct Context {
        let intent: String
        let direction: FundingDirection
        let amount: U128?
        let cash: FundingCash
    }
}
