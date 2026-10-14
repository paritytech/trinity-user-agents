import Foundation
import SubstrateSdk

public typealias WalletManaging = MetaAccountModelProtocol & RawKeypairProviding &
    SigningSecretProviding & TypedSigningProviding
