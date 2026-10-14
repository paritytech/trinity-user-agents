import Foundation
import ChainRegistry

protocol TokensInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol TokensOutputProtocol: AnyObject {
    func didReceive(chainAssets: [ChainAsset])
    func didReceive(error: TokensFetchError)
}

enum TokensFetchError: Error {
    case fetchFailed(Error)
}
