import Foundation
import TrUAPIHost

final class FundingSummaryInteractor: FundingEntryInteractor, FundingSummaryInteractorInputProtocol {
    func start(provider: ProductId) async throws {
        try await service.start(entryId, provider: provider)
    }
}
