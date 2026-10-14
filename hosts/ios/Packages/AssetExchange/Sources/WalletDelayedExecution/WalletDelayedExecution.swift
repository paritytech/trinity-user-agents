import Foundation
import SubstrateSdk

public protocol WalletDelayedExecVerifing {
    func executesCallWithDelay(_ wallet: MetaAccountModelProtocol, chain: ChainProtocol) -> Bool
}
