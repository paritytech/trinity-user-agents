import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Individuality

extension CoinagePallet {
    enum Storage {
        case consumedFreeUnloadTokens
        case recyclersCoinToRecycler
        case coinsByOwner
        case recyclerAliasStates
        case recyclersUnloadedCount
        case instances
    }
}

extension CoinagePallet.Storage: StoragePathConvertible {
    var name: String {
        switch self {
        case .recyclersCoinToRecycler:
            "RecyclersCoinToRecycler"
        case .consumedFreeUnloadTokens:
            "ConsumedFreeUnloadTokens"
        case .coinsByOwner:
            "CoinsByOwner"
        case .recyclerAliasStates:
            "RecyclerAliasStates"
        case .recyclersUnloadedCount:
            "RecyclersUnloadedCount"
        case .instances:
            "Instances"
        }
    }

    var moduleName: String { CoinagePallet.name }
}
