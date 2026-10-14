import Foundation

extension FiatOnrampTrackedTransactionStatus.Funding {
    init(fundingTransactionStatus: FiatOnrampTransactionStatus) {
        switch fundingTransactionStatus {
        case .pending:
            self = .inProgress
        case .settling:
            self = .inProgress
        case .settled:
            self = .completed
        case .failed:
            self = .failed
        case .unknown:
            self = .failed
        }
    }
}

extension FiatOnrampTransactionStatusPayload.Status {
    init(trackedTransactionStatus: FiatOnrampTrackedTransactionStatus) {
        switch trackedTransactionStatus {
        case .funding(.inProgress),
             .funding(.completed):
            self = .funding
        case .funding(.failed):
            self = .failed
        case let .swapping(swapping):
            switch swapping.status {
            case let .inProgress(remainingTime):
                self = .inProgress(
                    remainedTime: remainingTime,
                    amountIn: swapping.amountIn,
                    amountOut: swapping.amountOut
                )
            case .failed:
                self = .failed
            case .completed:
                self = .completed(
                    amountIn: swapping.amountIn,
                    amountOut: swapping.amountOut
                )
            }
        }
    }
}
