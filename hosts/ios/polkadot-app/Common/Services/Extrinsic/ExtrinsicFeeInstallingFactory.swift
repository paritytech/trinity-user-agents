import Foundation
import Operation_iOS
import ExtrinsicService
import HydrationSdk
import SubstrateSdk
import AssetHubSdk
import ChainRegistry

final class ExtrinsicFeeInstallingFactory {
    let host: ExtrinsicFeeEstimatorHostProtocol

    private lazy var hydraInstallingFactory = HydraExtrinsicFeeInstallerFactory(
        host: host,
        tokenConverter: HydrationTokenConverter()
    )

    init(host: ExtrinsicFeeEstimatorHostProtocol) {
        self.host = host
    }
}

private extension ExtrinsicFeeInstallingFactory {
    func createHydraFeeInstallingWrapper(
        chainAsset: ChainAsset,
        feeInstallingFactory: HydraExtrinsicFeeInstallerFactoryProtocol,
        accountClosure: @escaping () throws -> AccountProtocol
    ) -> CompoundOperationWrapper<ExtrinsicFeeInstalling> {
        OperationCombiningService.compoundNonOptionalWrapper(
            operationQueue: host.operationQueue
        ) {
            let account = try accountClosure()

            return feeInstallingFactory.createFeeInstaller(
                for: chainAsset,
                payerAccountId: account.accountId
            )
        }
    }
}

extension ExtrinsicFeeInstallingFactory: ExtrinsicFeeInstallingFactoryProtocol {
    func createFeeInstallerWrapper(
        chainAsset: ChainAssetProtocol,
        accountClosure: @escaping () throws -> AccountProtocol
    ) -> CompoundOperationWrapper<ExtrinsicFeeInstalling> {
        guard let localModel = chainAsset as? ChainAsset else {
            let error = ChainAssetConversionError.unsupportedAsset(chainAsset.chainAssetId.stringValue)
            return CompoundOperationWrapper.createWithError(error)
        }

        switch AssetType(rawType: localModel.asset.type) {
        case .statemine:
            return CompoundOperationWrapper.createWithResult(
                ExtrinsicAssetConversionFeeInstaller(
                    feeAsset: chainAsset,
                    tokenConverter: AssetHubTokenConverter()
                )
            )
        case .orml,
             .ormlHydrationEvm:
            return createHydraFeeInstallingWrapper(
                chainAsset: localModel,
                feeInstallingFactory: hydraInstallingFactory,
                accountClosure: accountClosure
            )
        case .none,
             .native:
            return CompoundOperationWrapper.createWithResult(ExtrinsicNativeFeeInstaller())
        }
    }
}
