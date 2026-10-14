import Foundation
import SubstrateSdk
import AssetExchange

public enum AssetHubExtrinsicConverterError: Error {
    case remoteAssetNotFound(ChainAssetId)
}

public protocol AssetHubExtrinsicConverting {
    func addingOperation(
        to builder: ExtrinsicBuilderProtocol,
        chain: ChainProtocol,
        args: AssetConversion.CallArgs,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> ExtrinsicBuilderProtocol
}

public final class AssetHubExtrinsicConverter {
    let tokenConverter: AssetHubTokenConverting

    public init(tokenConverter: AssetHubTokenConverting) {
        self.tokenConverter = tokenConverter
    }
}

extension AssetHubExtrinsicConverter: AssetHubExtrinsicConverting {
    public func addingOperation(
        to builder: ExtrinsicBuilderProtocol,
        chain: ChainProtocol,
        args: AssetConversion.CallArgs,
        codingFactory: RuntimeCoderFactoryProtocol
    ) throws -> ExtrinsicBuilderProtocol {
        guard
            let remoteAssetIn = tokenConverter.convertToMultilocation(
                chainAssetId: args.assetIn,
                chain: chain,
                codingFactory: codingFactory
            ) else {
            throw AssetHubExtrinsicConverterError.remoteAssetNotFound(args.assetIn)
        }

        guard
            let remoteAssetOut = tokenConverter.convertToMultilocation(
                chainAssetId: args.assetOut,
                chain: chain,
                codingFactory: codingFactory
            ) else {
            throw AssetHubExtrinsicConverterError.remoteAssetNotFound(args.assetOut)
        }

        switch args.direction {
        case .sell:
            let amountOutMin = args.amountOut - args.slippage.mul(value: args.amountOut)

            let call = AssetConversionPallet.SwapExactTokensForTokensCall(
                path: [remoteAssetIn, remoteAssetOut],
                amountIn: args.amountIn,
                amountOutMin: amountOutMin,
                sendTo: args.receiver,
                keepAlive: false
            )

            return try builder.adding(call: call.runtimeCall(for: AssetConversionPallet.name))
        case .buy:
            let amountInMax = args.amountIn + args.slippage.mul(value: args.amountIn)

            let call = AssetConversionPallet.SwapTokensForExactTokensCall(
                path: [remoteAssetIn, remoteAssetOut],
                amountOut: args.amountOut,
                amountInMax: amountInMax,
                sendTo: args.receiver,
                keepAlive: false
            )

            return try builder.adding(call: call.runtimeCall(for: AssetConversionPallet.name))
        }
    }
}
