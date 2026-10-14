import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

public struct HydraExchangeSwapParams {
    struct Params {
        let referral: AccountId?

        var shouldSetReferral: Bool {
            referral == nil
        }
    }

    enum Operation {
        case omniSell(HydraOmnipool.SellCall)
        case omniBuy(HydraOmnipool.BuyCall)
        case routedSell(HydraRouter.SellCall)
        case routedBuy(HydraRouter.BuyCall)
    }

    let params: Params
    let updateReferral: HydraDx.LinkReferralCodeCall?
    let swap: Operation
}

public protocol HydraExchangeExtrinsicParamsFactoryProtocol {
    func createOperationWrapper(
        for route: HydraDx.RemoteSwapRoute,
        callArgs: AssetConversion.CallArgs
    ) -> CompoundOperationWrapper<HydraExchangeSwapParams>
}

enum HydraExchangeExtrinsicParamsFactoryError: Error {
    case invalidReferralCode
}

final class HydraExchangeExtrinsicParamsFactory {
    let chain: ChainProtocol
    let swapService: HydraSwapParamsService
    let runtimeProvider: RuntimeCodingServiceProtocol
    let referralCode: String?
    let tokenConverter: HydrationTokenConverting

    init(
        chain: ChainProtocol,
        swapService: HydraSwapParamsService,
        runtimeProvider: RuntimeCodingServiceProtocol,
        tokenConverter: HydrationTokenConverting,
        referralCode: String?
    ) {
        self.chain = chain
        self.swapService = swapService
        self.runtimeProvider = runtimeProvider
        self.tokenConverter = tokenConverter
        self.referralCode = referralCode
    }

    private func createOperation(
        for remoteAssetIn: HydraDx.AssetId,
        remoteAssetOut: HydraDx.AssetId,
        callArgs: AssetConversion.CallArgs,
        route: HydraDx.RemoteSwapRoute
    ) -> HydraExchangeSwapParams.Operation {
        switch callArgs.direction {
        case .sell:
            let amountOutMin = callArgs.amountOut - callArgs.slippage.mul(value: callArgs.amountOut)

            if HydraExchangeExtrinsicConverter.isOmnipoolSwap(route: route) {
                return .omniSell(
                    HydraOmnipool.SellCall(
                        assetIn: remoteAssetIn,
                        assetOut: remoteAssetOut,
                        amount: callArgs.amountIn,
                        minBuyAmount: amountOutMin
                    )
                )
            } else {
                return .routedSell(
                    HydraRouter.SellCall(
                        assetIn: remoteAssetIn,
                        assetOut: remoteAssetOut,
                        amountIn: callArgs.amountIn,
                        minAmountOut: amountOutMin,
                        route: HydraExchangeExtrinsicConverter.convertRouteToTrade(route)
                    )
                )
            }
        case .buy:
            let amountInMax = callArgs.amountIn + callArgs.slippage.mul(value: callArgs.amountIn)

            if HydraExchangeExtrinsicConverter.isOmnipoolSwap(route: route) {
                return .omniBuy(
                    HydraOmnipool.BuyCall(
                        assetOut: remoteAssetOut,
                        assetIn: remoteAssetIn,
                        amount: callArgs.amountOut,
                        maxSellAmount: amountInMax
                    )
                )
            } else {
                return .routedBuy(
                    HydraRouter.BuyCall(
                        assetIn: remoteAssetIn,
                        assetOut: remoteAssetOut,
                        amountOut: callArgs.amountOut,
                        maxAmountIn: amountInMax,
                        route: HydraExchangeExtrinsicConverter.convertRouteToTrade(route)
                    )
                )
            }
        }
    }

    private func createSwapParams(
        from params: HydraExchangeSwapParams.Params,
        remoteAssetIn: HydraDx.AssetId,
        remoteAssetOut: HydraDx.AssetId,
        route: HydraDx.RemoteSwapRoute,
        callArgs: AssetConversion.CallArgs
    ) throws -> HydraExchangeSwapParams {
        let referralCall: HydraDx.LinkReferralCodeCall?

        if params.shouldSetReferral, let referralCode {
            let code = try referralCode.data(using: .utf8).mapOrThrow(
                HydraExchangeExtrinsicParamsFactoryError.invalidReferralCode
            )

            referralCall = .init(code: code)
        } else {
            referralCall = nil
        }

        let operation = createOperation(
            for: remoteAssetIn,
            remoteAssetOut: remoteAssetOut,
            callArgs: callArgs,
            route: route
        )

        return HydraExchangeSwapParams(
            params: params,
            updateReferral: referralCall,
            swap: operation
        )
    }

    private func createSwapOperationWrapper(
        assetIn: ChainAssetProtocol,
        assetOut: ChainAssetProtocol,
        route: HydraDx.RemoteSwapRoute,
        callArgs: AssetConversion.CallArgs,
        tokenConverter: HydrationTokenConverting
    ) -> CompoundOperationWrapper<HydraExchangeSwapParams> {
        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

        let swapParamsOperation = swapService.createFetchOperation()

        let mergeOperation = ClosureOperation<HydraExchangeSwapParams> {
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            let swapParams = try swapParamsOperation.extractNoCancellableResultData()

            let remoteAssetIn = try tokenConverter.convertToRemote(
                chainAsset: assetIn,
                codingFactory: codingFactory
            ).remoteAssetId

            let remoteAssetOut = try tokenConverter.convertToRemote(
                chainAsset: assetOut,
                codingFactory: codingFactory
            ).remoteAssetId

            let params = HydraExchangeSwapParams.Params(referral: swapParams.referralLink)

            return try self.createSwapParams(
                from: params,
                remoteAssetIn: remoteAssetIn,
                remoteAssetOut: remoteAssetOut,
                route: route,
                callArgs: callArgs
            )
        }

        mergeOperation.addDependency(codingFactoryOperation)
        mergeOperation.addDependency(swapParamsOperation)

        return CompoundOperationWrapper(
            targetOperation: mergeOperation,
            dependencies: [codingFactoryOperation, swapParamsOperation]
        )
    }
}

extension HydraExchangeExtrinsicParamsFactory: HydraExchangeExtrinsicParamsFactoryProtocol {
    func createOperationWrapper(
        for route: HydraDx.RemoteSwapRoute,
        callArgs: AssetConversion.CallArgs
    ) -> CompoundOperationWrapper<HydraExchangeSwapParams> {
        do {
            let assetIn = try chain.chainAssetInterfaceOrError(for: callArgs.assetIn.assetId)

            let assetOut = try chain.chainAssetInterfaceOrError(for: callArgs.assetOut.assetId)

            return createSwapOperationWrapper(
                assetIn: assetIn,
                assetOut: assetOut,
                route: route,
                callArgs: callArgs,
                tokenConverter: tokenConverter
            )
        } catch {
            return .createWithError(error)
        }
    }
}
