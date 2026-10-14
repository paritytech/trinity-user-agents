import Foundation
import Operation_iOS
import AssetsManagement
import ChainRegistry

typealias BalanceProcessorCallback = (AssetBalance) -> Void

final class CallbackBalanceUpdateProcessor: BaseBalanceUpdateProcessor {
    let callbackQueue: DispatchQueue
    let callbackClosure: BalanceProcessorCallback

    init(
        transactionHandler: TransactionSubscribing?,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping BalanceProcessorCallback
    ) {
        self.callbackQueue = callbackQueue
        self.callbackClosure = callbackClosure

        super.init(transactionHandler: transactionHandler)
    }

    private var lastBalances: [String: AssetBalance] = [:]
    private let mutex = NSLock()

    override func saveBalance(_ balance: AssetBalance) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard lastBalances[balance.identifier] != balance else {
            return
        }

        lastBalances[balance.identifier] = balance

        dispatchInQueueWhenPossible(callbackQueue) { [weak self] in
            self?.callbackClosure(balance)
        }
    }
}
