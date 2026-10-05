import Foundation
import SubstrateSdk

public enum AllocatableResource: Equatable {
    case statementStoreAllowance
    case bulletInAllowance
    case smartContractAllowance(dest: ProductAccountSelector)
    case autoSigning
}

public enum AllocationOutcome: Equatable {
    case allocated(AllocatedResource)
    case rejected
    case notAvailable
}

public enum AllocatedResource: Equatable {
    case autoSigning(AutoSigningSecrets)
    case statementStoreAllowance(privateKey: Data)
    case bulletInAllowance(privateKey: Data)
    case smartContractAllowance
}

public enum AutoSigningSecretsError: Error, Equatable {
    case invalidKeyLength(expected: Int, actual: Int)
    case invalidEntropyLength(expected: Int, actual: Int)
}

/// `AutoSigning` payload: the 64-byte expanded sr25519 secret
/// (key ++ nonce) of `//product//{productId}` — enough to sign and soft-derive
/// every account in the product subtree, and nothing beyond it. It also carries the
/// 32-byte entropy of the product's ring-VRF domain.
public struct AutoSigningSecrets: Equatable {
    public static let privateKeyLength = 64
    public static let ringVrfDomainEntropyLength = 32

    public let productRootPrivateKey: Data
    public let ringVrfDomainEntropy: Data

    public init(productRootPrivateKey: Data, ringVrfDomainEntropy: Data) throws {
        guard productRootPrivateKey.count == Self.privateKeyLength else {
            throw AutoSigningSecretsError.invalidKeyLength(
                expected: Self.privateKeyLength,
                actual: productRootPrivateKey.count
            )
        }

        guard ringVrfDomainEntropy.count == Self.ringVrfDomainEntropyLength else {
            throw AutoSigningSecretsError.invalidEntropyLength(
                expected: Self.ringVrfDomainEntropyLength,
                actual: ringVrfDomainEntropy.count
            )
        }

        self.productRootPrivateKey = productRootPrivateKey
        self.ringVrfDomainEntropy = ringVrfDomainEntropy
    }
}
