import Foundation
import SubstrateSdk
import KeyDerivation

public enum SlotHolderSignerError: Error, Equatable {
    case accountMismatch(signerKeyId: String)
}

public protocol SlotHolderSignerResolving {
    func signer(for signerKeyId: String, expecting accountId: AccountId) throws -> any TypedSigningProviding
}
