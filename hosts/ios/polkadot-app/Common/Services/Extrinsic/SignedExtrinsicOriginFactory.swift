import Foundation
import ExtrinsicService
import SubstrateSdk
import Individuality
import KeyDerivation

final class SignedExtrinsicOriginFactory: ExtrinsicOriginFactory {}

extension SignedExtrinsicOriginFactory: ExtrinsicOriginDefiningFactoryProtocol {
    func extrinsicOriginDefiner(
        from wallet: MetaAccountModelProtocol,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining {
        guard let manager = wallet as? WalletManaging else {
            throw ExtrinsicOriginFactoryError.unexpectedWalletModel
        }

        let accountOrigin = try createAccountOrigin(for: manager, chain: chain)

        let feeModifier = try createFeeModifier(for: chain)

        let signedOrigin = try createSigningByAccountOrigin(for: manager, chain: chain)

        let restrictionOrigin = RestrictsOriginDefinition(enabled: false)

        return ExtrinsicCompoundOrigin(children: [accountOrigin, feeModifier, restrictionOrigin, signedOrigin])
    }
}
