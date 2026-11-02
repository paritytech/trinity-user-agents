import BackgroundExecution
import Foundation
import Revive
import StructuredConcurrency

struct ProductLoadErrorClassifier: RetryErrorClassifying {
    private let network = NetworkErrorClassifier()

    func isRetryable(_ error: Error) -> Bool {
        switch error {
        case is BackgroundExecutionExpiredError,
             is ProductUpdateFetchError:
            true
        case let contractError as DotNsContractError:
            Self.isRetryable(contractError)
        default:
            network.isRetryable(error)
        }
    }
}

private extension ProductLoadErrorClassifier {
    static func isRetryable(_ contractError: DotNsContractError) -> Bool {
        guard case let .contractCallFailed(underlying) = contractError else {
            return false
        }

        switch underlying {
        case is ReviveContractRevertedError,
             is ReviveContractError,
             is CancellationError:
            return false
        default:
            return true
        }
    }
}
