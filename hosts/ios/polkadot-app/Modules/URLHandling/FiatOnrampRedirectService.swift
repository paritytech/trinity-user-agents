import Foundation

class FiatOnrampRedirectService {
    let host = "fiatOnramp"
    let buySuccessPath = "/buySuccess"

    let fiatOnrampTransactionTracking: FiatOnrampTrackingServiceProtocol

    init(fiatOnrampTransactionTracking: FiatOnrampTrackingServiceProtocol) {
        self.fiatOnrampTransactionTracking = fiatOnrampTransactionTracking
    }
}

extension FiatOnrampRedirectService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard url.host() == host else {
            return false
        }

        if url.path() == buySuccessPath {
            return handleBuySuccess(url)
        }

        return true
    }

    private func handleBuySuccess(_ url: URL) -> Bool {
        guard
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            let queryItems = components.queryItems,
            let sessionId = queryItems.first(where: { $0.name == "sessionId" })?.value
        else {
            return false
        }
        fiatOnrampTransactionTracking.handleBuySuccess(for: FiatOnRampSessionId(value: sessionId))
        return true
    }
}
