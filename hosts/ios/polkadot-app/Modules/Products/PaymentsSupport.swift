import Coinage

/// The wallet as the legacy product host API pays and tops up through it.
final class PaymentsSupport {
    let coinageService: CoinageServicing

    init(coinageService: CoinageServicing) {
        self.coinageService = coinageService
    }
}
