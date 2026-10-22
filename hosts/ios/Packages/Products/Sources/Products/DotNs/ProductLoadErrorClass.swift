import Foundation
import FoundationExt
import Revive

public enum ProductLoadErrorClass: Equatable, Sendable {
    case transient
    case permanent
    case cancelled

    public init(_ error: Error) {
        switch error {
        case is CancellationError:
            self = .cancelled
        case let urlError as URLError:
            self = Self.classify(urlError)
        case let httpError as HTTPResponseError:
            self = Self.classify(httpStatus: httpError.statusCode)
        case let contractError as DotNsContractError:
            self = Self.classify(contractError)
        default:
            self = .permanent
        }
    }
}

private extension ProductLoadErrorClass {
    static let transientUrlErrorCodes: Set<URLError.Code> = [
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

    static func classify(_ urlError: URLError) -> ProductLoadErrorClass {
        if urlError.code == .cancelled {
            return .cancelled
        }

        return transientUrlErrorCodes.contains(urlError.code) ? .transient : .permanent
    }
    static func classify(httpStatus: Int) -> ProductLoadErrorClass {
        switch httpStatus {
        case 408,
             425,
             429,
             500 ... 599:
            .transient
        default:
            .permanent
        }
    }
    static func classify(_ contractError: DotNsContractError) -> ProductLoadErrorClass {
        guard case let .contractCallFailed(underlying) = contractError else {
            return .permanent
        }

        switch underlying {
        case is ReviveContractRevertedError,
             is ReviveContractError:
            return .permanent
        case is CancellationError:
            return .cancelled
        default:
            return .transient
        }
    }
}
