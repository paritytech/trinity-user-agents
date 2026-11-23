import Foundation
import Individuality
import KeyDerivation
import SubstrateSdk

final class DerivationPathSignerResolver: SlotHolderSignerResolving {
    private let entropyManager: RootEntropyManaging

    init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }

    func signer(for signerKeyId: String, expecting accountId: AccountId) throws -> any TypedSigningProviding {
        let wallet = DynamicDerivedWallet(derivationPath: signerKeyId, entropyManager: entropyManager)

        guard try wallet.getRawPublicKey() == accountId else {
            throw SlotHolderSignerError.accountMismatch(signerKeyId: signerKeyId)
        }

        return wallet
    }
}
