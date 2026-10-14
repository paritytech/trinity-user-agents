import Foundation
import Coinage

#if TESTNET_FEATURE
    /// Debug information about the transfer strategy
    struct TransferStrategyDebugInfo: Equatable {
        enum StrategyType: String {
            case exactMatch = "ExactMatch"
            case split = "Split"
            case unloadAndSplit = "UnloadAndSplit"
            case externalPayment = "ExternalPayment"
        }

        struct CoinInfo: Equatable {
            let derivationIndex: CoinageKeyIndex
            let exponent: Int16
        }

        struct VoucherInfo: Equatable {
            let derivationIndex: CoinageKeyIndex
            let exponent: Int16
        }

        struct SplitInfo: Equatable {
            let overflowCoin: CoinInfo
            let targetDenominations: [Int16]
            let changeDenominations: [Int16]
        }

        let strategyType: StrategyType
        let coinsUsed: [CoinInfo]
        let splitInfo: SplitInfo?
        let vouchersToUnload: [VoucherInfo]
    }
#endif
