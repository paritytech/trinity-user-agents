import Foundation
import ChainRegistry

@MainActor
class TokensPresenter {
    let tokensInteractor: TokensInputProtocol
    let logger: LoggerProtocol

    private(set) var chainAssets: [ChainAsset]?

    init(tokensInteractor: TokensInputProtocol, logger: LoggerProtocol) {
        self.tokensInteractor = tokensInteractor
        self.logger = logger
    }

    func didReceive(chainAssets: [ChainAsset]) {
        self.chainAssets = chainAssets

        logger.debug("Did receive tokens: \(chainAssets.map(\.asset.symbol))")
    }

    func didReceive(error: TokensFetchError) {
        logger.debug("Error: \(error)")
    }
}

extension TokensPresenter: TokensOutputProtocol {}
