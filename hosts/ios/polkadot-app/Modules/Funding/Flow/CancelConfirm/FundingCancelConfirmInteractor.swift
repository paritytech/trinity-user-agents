import Foundation
import TrUAPIHost

final class FundingCancelConfirmInteractor: FundingEntryInteractor, FundingCancelConfirmInteractorInputProtocol {
    func cancel() async throws -> CancelOutcome {
        try await service.cancel(entryId)
    }
}
