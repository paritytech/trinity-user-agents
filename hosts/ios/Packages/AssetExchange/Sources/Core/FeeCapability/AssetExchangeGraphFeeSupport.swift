import Foundation
import Operation_iOS
import SubstrateSdk

public protocol AssetExchangeFeeSupporting {
    func canPayFee(inNonNative chainAssetId: ChainAssetId) -> Bool
}

public protocol AssetExchangeFeeSupportFetching {
    var identifier: String { get }

    func createFeeSupportWrapper() -> CompoundOperationWrapper<AssetExchangeFeeSupporting>
}

public protocol AssetExchangeFeeSupportFetchersProviding {
    func setup()
    func throttle()

    func subscribeFeeFetchers(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping ([AssetExchangeFeeSupportFetching]) -> Void
    )

    func unsubscribeFeeFetchers(_ target: AnyObject)
}

public protocol AssetsExchangeFeeSupportProviding {
    func setup()
    func throttle()

    func subscribeFeeSupport(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping (AssetExchangeFeeSupporting?) -> Void
    )

    func unsubscribe(_ target: AnyObject)

    func fetchCurrentState(in queue: DispatchQueue, completionClosure: @escaping (AssetExchangeFeeSupporting?) -> Void)
}
