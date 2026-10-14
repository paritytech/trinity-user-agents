import Coinage

final class PaymentsSupport {
    let coinageService: CoinageServicing

    var externalPaymentService: ExternalPaymentServicing {
        coinageService.externalPaymentService
    }

    var incomingPaymentService: IncomingPaymentServicing {
        coinageService.incomingPaymentService
    }

    init(coinageService: CoinageServicing) {
        self.coinageService = coinageService
    }
}
