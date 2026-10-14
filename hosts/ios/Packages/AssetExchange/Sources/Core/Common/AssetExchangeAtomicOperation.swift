import Foundation
import Operation_iOS
import SubstrateSdk
import ExtrinsicService

public protocol AssetExchangeAtomicOperationProtocol {
    var swapLimit: AssetExchangeSwapLimit { get }

    func executeWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<Balance>

    func submitWrapper(
        for swapLimit: AssetExchangeSwapLimit,
        creditingTo accountId: AccountId?
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel>

    func estimateFee(creditingTo accountId: AccountId?) -> CompoundOperationWrapper<AssetExchangeOperationFee>

    func requiredAmountToGetAmountOut(
        _ amountOutClosure: @escaping () throws -> Balance
    ) -> CompoundOperationWrapper<Balance>
}

extension AssetExchangeAtomicOperationProtocol {
    func estimateFee() -> CompoundOperationWrapper<AssetExchangeOperationFee> {
        estimateFee(creditingTo: nil)
    }

    func executeWrapper(
        for swapLimit: AssetExchangeSwapLimit
    ) -> CompoundOperationWrapper<Balance> {
        executeWrapper(for: swapLimit, creditingTo: nil)
    }

    func submitWrapper(
        for swapLimit: AssetExchangeSwapLimit
    ) -> CompoundOperationWrapper<ExtrinsicSubmittedModel> {
        submitWrapper(for: swapLimit, creditingTo: nil)
    }
}
