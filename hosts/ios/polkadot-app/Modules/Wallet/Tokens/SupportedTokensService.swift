import Foundation
import SubstrateSdk

typealias SupportedTokensClosure = (Result<[ChainAssetId], Error>) -> Void

protocol SupportedTokensServiceProtocol {
    func fetchAvailableTokens(
        runningCompletionIn queue: DispatchQueue,
        completion: @escaping SupportedTokensClosure
    )

    func prefetchTokens()
}

final class SupportedTokensService {
    let supportedTokens: [ChainAssetId]

    init(supportedTokens: [ChainAssetId] = AppConfig.Assets.all) {
        self.supportedTokens = supportedTokens
    }
}

extension SupportedTokensService: SupportedTokensServiceProtocol {
    func prefetchTokens() {}

    func fetchAvailableTokens(runningCompletionIn queue: DispatchQueue, completion: @escaping SupportedTokensClosure) {
        queue.async { [weak self] in
            guard let self else {
                return
            }

            completion(.success(supportedTokens))
        }
    }
}
