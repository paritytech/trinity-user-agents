import Foundation
import SubstrateSdk
import AssetsManagement

protocol BalanceUpdateProcessing {
    func process(balance: AssetBalance, blockHash: BlockHashData?)
}
