import Foundation
import SubstrateSdk
import Operation_iOS
import AssetsManagement

class BaseBalanceUpdateProcessor {
    let transactionHandler: TransactionSubscribing?

    init(transactionHandler: TransactionSubscribing?) {
        self.transactionHandler = transactionHandler
    }

    func saveBalance(_: AssetBalance) {
        fatalError("Must be overriden by subclass")
    }
}

extension BaseBalanceUpdateProcessor: BalanceUpdateProcessing {
    func process(balance: AssetBalance, blockHash: BlockHashData?) {
        saveBalance(balance)

        if let blockHash {
            transactionHandler?.process(blockHash: blockHash)
        }
    }
}
