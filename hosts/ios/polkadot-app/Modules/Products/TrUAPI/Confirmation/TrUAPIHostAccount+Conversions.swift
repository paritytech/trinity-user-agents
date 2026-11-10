import Foundation
import Products
import KeyDerivation
import TrUAPIHost

enum TrUAPIReviewMappingError: Error, Equatable {
    case invalidDerivationIndexLength(Int)
    case notASigningReview
}

// MARK: - TrUAPIHost account conversions

extension TrUAPIHostProductAccountId {
    func toAppAccount() throws -> Products.ProductAccountId {
        try Products.ProductAccountId(
            productId: dotNsIdentifier,
            derivationIndex: derivationIndex.toSelector()
        )
    }
}

extension TrUAPIHostDerivationIndex {
    func toSelector() throws -> ProductAccountSelector {
        switch self {
        case let .index(value):
            return .index(value)
        case let .raw(bytes):
            guard bytes.count == DerivationIndex32.length else {
                throw TrUAPIReviewMappingError.invalidDerivationIndexLength(bytes.count)
            }
            return .raw(bytes)
        }
    }

    /// The 32-byte derivation index this selector expands to.
    func toIndex32() throws -> DerivationIndex32 {
        switch self {
        case let .index(value):
            DerivationIndex32(index: value)
        case let .raw(bytes):
            try DerivationIndex32(raw: bytes)
        }
    }
}
