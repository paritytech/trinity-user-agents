import Foundation
import FoundationExt
import TrUAPIHost

final class FundingProgressInteractor: FundingEntryInteractor, FundingProgressInteractorInputProtocol {
    weak var presenter: FundingProgressInteractorOutputProtocol?

    private let clock: any Clock<Duration>
    private var longerAtMs: UInt64?
    private var longerTask: Task<Void, Never>?

    init(service: FundingServiceProtocol, entryId: String, clock: any Clock<Duration> = ContinuousClock()) {
        self.clock = clock
        super.init(service: service, entryId: entryId)
    }

    deinit {
        longerTask?.cancel()
    }

    func refreshWhenTakingLonger(atMs longerAtMs: UInt64?) {
        guard longerAtMs != self.longerAtMs else { return }

        self.longerAtMs = longerAtMs
        longerTask?.cancel()
        guard let longerAtMs else { return }

        let wait = Date(milliseconds: longerAtMs).timeIntervalSinceNow
        longerTask = Task { [weak self, clock] in
            if wait > 0 {
                guard (try? await clock.sleep(for: .seconds(wait))) != nil else { return }
            }
            guard !Task.isCancelled else { return }
            self?.refresh()
        }
    }

    func answerCancelOffer(accepting terms: CancelTerms?) {
        Task { [weak self, service, entryId] in
            let result: Result<CancelConfirmation, Error>
            do {
                result = try await .success(service.answerCancelOffer(entryId, accepting: terms))
            } catch {
                result = .failure(error)
            }
            self?.presenter?.didReceiveCancelOfferAnswer(result: result)
        }
    }

    func acknowledge() {
        Task { [service, entryId] in await service.acknowledge(entryId) }
    }
}
