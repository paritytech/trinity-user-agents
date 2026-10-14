import Foundation
import SubstrateSdk

public typealias BalanceChangeDetectingClosure = (Result<BlockHashData, Error>) -> Void

public protocol BalanceChangeDetecting {
    func subscribe(
        notifyingIn queue: DispatchQueue,
        closure: @escaping BalanceChangeDetectingClosure
    )

    func unsubscribe()
}

public protocol BalanceChangeDetectorFactoryProtocol {
    func createDetector(
        for accountId: AccountId,
        chainAsset: ChainAssetProtocol
    ) -> BalanceChangeDetecting?
}
