import Foundation
import SubstrateSdkExt

extension CoinagePallet {
    enum ViewFunction {
        case getFreeUnloadTokens
        case maxFreeUnloadTokensPerTimePeriod
    }
}

extension CoinagePallet.ViewFunction: ViewFunctionCallConvertible {
    var name: String {
        switch self {
        case .getFreeUnloadTokens:
            "get_free_unload_token_info"
        case .maxFreeUnloadTokensPerTimePeriod:
            "get_max_free_unload_tokens_per_time_period"
        }
    }

    var moduleName: String { CoinagePallet.name }
}
