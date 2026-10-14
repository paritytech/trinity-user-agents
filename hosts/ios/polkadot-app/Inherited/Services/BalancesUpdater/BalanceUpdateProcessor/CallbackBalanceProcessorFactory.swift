import Foundation
import SubstrateSdk

final class CallbackBalanceProcessorFactory {
    let sharedProcessor: CallbackBalanceUpdateProcessor

    init(callbackQueue: DispatchQueue, callbackClosure: @escaping BalanceProcessorCallback) {
        sharedProcessor = CallbackBalanceUpdateProcessor(
            transactionHandler: nil,
            callbackQueue: callbackQueue,
            callbackClosure: callbackClosure
        )
    }
}

extension CallbackBalanceProcessorFactory: BalanceUpdateProcessorFactoryProtocol {
    func createProcessor(for _: AccountId, chainAssetId _: ChainAssetId) -> BalanceUpdateProcessing {
        sharedProcessor
    }
}
