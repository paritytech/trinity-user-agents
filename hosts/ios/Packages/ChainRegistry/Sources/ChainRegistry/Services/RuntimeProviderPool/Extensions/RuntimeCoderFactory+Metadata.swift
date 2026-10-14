import Foundation
import SubstrateSdk

public extension RuntimeCoderFactoryProtocol {
    func atLeastV15Runtime() -> Bool {
        if metadata is RuntimeMetadata || metadata is RuntimeMetadataV14 {
            false
        } else {
            true
        }
    }

    func supportsMetadataHash() -> Bool {
        let hasSignedExtension = metadata.getSignedExtensions().contains(
            Extrinsic.TransactionExtensionId.checkMetadataHash
        )

        return atLeastV15Runtime() && hasSignedExtension
    }
}
