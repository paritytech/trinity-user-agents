import Foundation
import SubstrateSdk
import SubstrateSdkExt

extension BabePallet {
    enum Storage {
        case currentEpoch
        case currentSlot
        case genesisSlot
    }
}

extension BabePallet.Storage: StoragePathConvertible {
    var moduleName: String {
        BabePallet.name
    }

    var name: String {
        switch self {
        case .currentEpoch:
            "EpochIndex"
        case .currentSlot:
            "CurrentSlot"
        case .genesisSlot:
            "GenesisSlot"
        }
    }
}
