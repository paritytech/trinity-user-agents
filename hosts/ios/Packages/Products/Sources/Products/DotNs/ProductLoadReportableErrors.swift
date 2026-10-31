import Foundation
import IssueMonitoring

extension DotNsContractError: ReportableError {
    public var reportCode: String {
        switch self {
        case .contentHashNotFound: "contentHashNotFound"
        case .contentHashTooShort: "contentHashTooShort"
        case .unsupportedEip1577Prefix: "unsupportedEip1577Prefix"
        case .contractCallFailed: "contractCallFailed"
        case .tldNotFound: "tldNotFound"
        }
    }
}

extension DotNsResolverError: ReportableError {
    public var reportCode: String {
        switch self {
        case .resolutionFailed: "resolutionFailed"
        case .contentHashNotFound: "contentHashNotFound"
        case .carParseFailed: "carParseFailed"
        case .storageFailed: "storageFailed"
        }
    }
}

extension CarFetcherError: ReportableError {
    public var reportCode: String {
        switch self {
        case .invalidContentHash: "invalidContentHash"
        case .notCarArchive: "notCarArchive"
        }
    }
}

extension DotNsContentStorageError: ReportableError {
    public var reportCode: String {
        switch self {
        case .pathEscapesContentDirectory: "pathEscapesContentDirectory"
        case .invalidContentHash: "invalidContentHash"
        }
    }
}

extension ProductResolutionError: ReportableError {
    public var reportCode: String {
        switch self {
        case .malformedManifest: "malformedManifest"
        }
    }
}

extension ProductUpdateFetchError: ReportableError {
    public var reportCode: String {
        switch self {
        case .tldUnavailable: "tldUnavailable"
        }
    }
}
