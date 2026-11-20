import Foundation
import TrUAPIHost

final class FundingProgressInteractor: FundingEntryInteractor, FundingProgressInteractorInputProtocol {
    func answerCancelOffer(accepting terms: CancelTerms?) async throws -> CancelConfirmation {
        try await service.answerCancelOffer(entryId, accepting: terms)
    }

    func acknowledge() {
        Task { [service, entryId] in await service.acknowledge(entryId) }
    }
}
