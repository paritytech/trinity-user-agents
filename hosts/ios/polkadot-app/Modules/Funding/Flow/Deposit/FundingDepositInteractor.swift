import Foundation

final class FundingDepositInteractor: FundingEntryInteractor, FundingDepositInteractorInputProtocol {
    func stopStarting() {
        service.stopStarting(entryId)
    }
}
