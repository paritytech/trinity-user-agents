import Foundation

final class FundingTokenInteractor: FundingEntryInteractor, FundingTokenInteractorInputProtocol {
    func startWhenQuoted() async throws {
        try await service.start(entryId, provider: nil)
    }
}
