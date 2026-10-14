import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Individuality
import BigInt

extension CoinagePallet {
    enum Constants {
        case maxConsolidation
        case maxSplitOutputs
        case maximumExponent
        case minimumExponent
        case unloadTokenTimePeriod
        case maxBatchUnpaidLoad
    }
}

extension CoinagePallet.Constants: ConstantPathConvertible {
    var name: String {
        switch self {
        case .maxConsolidation:
            "MaxConsolidation"
        case .maxSplitOutputs:
            "MaxSplitOutputs"
        case .maximumExponent:
            "MaximumExponent"
        case .minimumExponent:
            "MinimumExponent"
        case .unloadTokenTimePeriod:
            "UnloadTokenTimePeriodPeopleLitePeople"
        case .maxBatchUnpaidLoad:
            "MaxBatchUnpaidLoad"
        }
    }

    var moduleName: String { CoinagePallet.name }
}
