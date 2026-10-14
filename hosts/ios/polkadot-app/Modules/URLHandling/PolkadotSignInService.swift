import Foundation

protocol PolkadotSignInServiceOutputProtocol: AnyObject {
    func didReceiveSignInUrl(_ url: URL)
}

final class PolkadotSignInService {
    let host = "pair"

    weak var output: PolkadotSignInServiceOutputProtocol?

    private let polkadotHandshakeService: PolkadotHandshakeServicing
    private let logger: LoggerProtocol

    init(
        polkadotHandshakeService: PolkadotHandshakeServicing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.polkadotHandshakeService = polkadotHandshakeService
        self.logger = logger
    }
}

extension PolkadotSignInService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard url.host() == host else {
            return false
        }

        output?.didReceiveSignInUrl(url)
        return true
    }
}
