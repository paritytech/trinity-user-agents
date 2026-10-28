import Foundation
import FoundationExt

public protocol RetryErrorClassifying: Sendable {
    func isRetryable(_ error: Error) -> Bool
}

public struct NetworkErrorClassifier: RetryErrorClassifying {
    private static let retryableUrlErrorCodes: Set<URLError.Code> = [
        .timedOut,
        .networkConnectionLost,
        .notConnectedToInternet,
        .cannotConnectToHost,
        .cannotFindHost,
        .dnsLookupFailed,
        .secureConnectionFailed,
        .dataNotAllowed,
        .internationalRoamingOff,
        .callIsActive,
        .backgroundSessionWasDisconnected
    ]

    public init() {}

    public func isRetryable(_ error: Error) -> Bool {
        switch error {
        case let urlError as URLError:
            Self.retryableUrlErrorCodes.contains(urlError.code)
        case let httpError as HTTPResponseError:
            Self.isRetryable(httpStatus: httpError.statusCode)
        default:
            false
        }
    }
}

private extension NetworkErrorClassifier {
    static func isRetryable(httpStatus: Int) -> Bool {
        switch httpStatus {
        case 408,
             425,
             429,
             500 ... 599:
            true
        default:
            false
        }
    }
}
