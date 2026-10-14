import Foundation
import ExtrinsicService
import SubstrateSdk
import Individuality
import KeyDerivation

final class PersonLiteOriginFactory: ExtrinsicOriginFactory {}

extension PersonLiteOriginFactory: ExtrinsicOriginDefiningFactoryProtocol {
    func extrinsicOriginDefiner(
        from wallet: MetaAccountModelProtocol,
        chain: ChainProtocol
    ) throws -> ExtrinsicOriginDefining {
        guard let manager = wallet as? WalletManaging else {
            throw ExtrinsicOriginFactoryError.unexpectedWalletModel
        }

        let accountOrigin = try createAccountOrigin(for: manager, chain: chain)

        let signedOrigin = try createSigningByAccountOrigin(for: manager, chain: chain)

        let feeModifier = try createFeeModifier(for: chain)

        let lightPersonOrigin = PeopleLiteOriginDefinition()

        let restrictionOrigin = RestrictsOriginDefinition(enabled: true)

        return ExtrinsicCompoundOrigin(children: [
            accountOrigin,
            feeModifier,
            restrictionOrigin,
            lightPersonOrigin,
            signedOrigin
        ])
    }
}
