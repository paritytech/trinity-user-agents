import Foundation
import SubstrateSdk
import SubstrateSdkExt

public extension AccountAddress {
    func accountIdOrDummy(for chain: ChainModel) -> AccountId {
        (try? toAccountId(using: chain.chainFormat)) ?? AccountId.zeroAccountId(of: chain.accountIdSize)
    }
}

public extension ChainModel {
    var chainFormat: ChainFormat {
        if isEthereumBased {
            .ethereum
        } else {
            .substrate(addressPrefix)
        }
    }
}
