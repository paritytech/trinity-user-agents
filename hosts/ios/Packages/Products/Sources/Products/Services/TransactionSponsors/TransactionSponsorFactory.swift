import Foundation

public protocol TransactionSponsorMaking {
    func makePreimageSponsor() -> PreimageSubmitSponsoring
    func makeStatementStoreSponsor() -> StatementStoreSponsoring
    func makePGasSponsor() -> PGasTransactionSponsoring
}
