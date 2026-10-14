import Foundation
import KeyDerivation
import SubstrateSdk

public enum ProductRootEntropyDeriverError: Error, Equatable {
    case keyTooLarge(maxSize: Int, actualSize: Int)
}

public protocol ProductRootEntropyDeriving {
    func deriveEntropy(productId: ProductId, key: Data) throws -> Data
}

public final class ProductRootEntropyDeriver {
    static let maxKeySize = 32

    private let rootEntropySourceDeriver: any RootEntropySourceDeriving

    public init(rootEntropySourceDeriver: any RootEntropySourceDeriving) {
        self.rootEntropySourceDeriver = rootEntropySourceDeriver
    }

    public convenience init(entropyManager: RootEntropyManaging) {
        self.init(rootEntropySourceDeriver: RootEntropySourceDeriver(entropyManager: entropyManager))
    }
}

extension ProductRootEntropyDeriver: ProductRootEntropyDeriving {
    public func deriveEntropy(productId: ProductId, key: Data) throws -> Data {
        guard key.count <= Self.maxKeySize else {
            throw ProductRootEntropyDeriverError.keyTooLarge(
                maxSize: Self.maxKeySize,
                actualSize: key.count
            )
        }

        let rootEntropySource = try rootEntropySourceDeriver.deriveRootEntropySource()
        let productIdHash = try Data(productId.utf8).blake2b32()
        let perProductEntropy = try rootEntropySource.blake2b32WithKey(productIdHash)
        return try perProductEntropy.blake2b32WithKey(key)
    }
}
