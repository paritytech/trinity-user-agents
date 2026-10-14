import Foundation
import SubstrateSdk
import BigInt
import ExtrinsicService

enum ExtrinsicAssetConversionFeeInstallError: Error {
    case invalidAssetId
}

public final class ExtrinsicAssetConversionFeeInstaller {
    let feeAsset: ChainAssetProtocol
    let tip: BigUInt
    let tokenConverter: AssetHubTokenConverting

    public init(
        feeAsset: ChainAssetProtocol,
        tip: BigUInt = 0,
        tokenConverter: AssetHubTokenConverting
    ) {
        self.feeAsset = feeAsset
        self.tip = tip
        self.tokenConverter = tokenConverter
    }
}

extension ExtrinsicAssetConversionFeeInstaller: ExtrinsicFeeInstalling {
    public func installingFeeSettings(
        to builder: ExtrinsicBuilderProtocol,
        coderFactory: RuntimeCoderFactoryProtocol
    ) throws -> ExtrinsicBuilderProtocol {
        guard
            let assetId = tokenConverter.convertToMultilocation(
                chainAsset: feeAsset,
                codingFactory: coderFactory
            ) else {
            throw ExtrinsicAssetConversionFeeInstallError.invalidAssetId
        }

        return builder.adding(
            transactionExtension: TransactionExtension.ChargeAssetTxPayment(
                tip: tip,
                assetId: assetId
            )
        )
    }
}
