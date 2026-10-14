import Foundation
import Products
import UIKitExt

/// Turns product load failures into messages that say what went wrong: an unregistered name, an
/// unreachable gateway, or content this build cannot open.
extension DotNsResolverError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        switch self {
        case .resolutionFailed,
             .contentHashNotFound:
            .productLoadNotFound
        case let .carParseFailed(error),
             let .storageFailed(error):
            error.isConnectionError ? .productLoadUnreachable : .productLoadDamaged
        }
    }
}

extension DotNsContractError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        switch self {
        case .contentHashNotFound,
             .tldNotFound:
            .productLoadNotFound
        case .contentHashTooShort,
             .unsupportedEip1577Prefix:
            .productLoadDamaged
        case .contractCallFailed:
            .productLoadUnreachable
        }
    }
}

extension CarFetcherError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        switch self {
        case .invalidContentHash,
             .notCarArchive:
            .productLoadDamaged
        }
    }
}

extension DotNsContentStorageError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        switch self {
        case .pathEscapesContentDirectory,
             .invalidContentHash:
            .productLoadDamaged
        }
    }
}

extension ProductResolutionError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        switch self {
        case .malformedManifest:
            .productLoadDamaged
        }
    }
}

extension ErrorContent {
    static var productLoadNotFound: ErrorContent {
        ErrorContent(
            title: String(localized: .Products.productLoadNotFoundTitle),
            message: String(localized: .Products.productLoadNotFoundMessage)
        )
    }

    static var productLoadUnreachable: ErrorContent {
        ErrorContent(
            title: String(localized: .Products.productLoadUnreachableTitle),
            message: String(localized: .Products.productLoadUnreachableMessage)
        )
    }

    static var productLoadDamaged: ErrorContent {
        ErrorContent(
            title: String(localized: .Products.productLoadDamagedTitle),
            message: String(localized: .Products.productLoadDamagedMessage)
        )
    }
}
