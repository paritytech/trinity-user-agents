import Foundation
import SubstrateSdk
import ExtrinsicService
import Operation_iOS

public protocol HydraExtrinsicFeeInstallerFactoryProtocol {
    func createFeeInstaller(
        for chainAsset: ChainAssetProtocol,
        payerAccountId: AccountId
    ) -> CompoundOperationWrapper<ExtrinsicFeeInstalling>
}

public final class HydraExtrinsicFeeInstallerFactory {
    let host: ExtrinsicFeeEstimatorHostProtocol
    let tokenConverter: HydrationTokenConverting

    private var feeCurrencyService: HydraSwapFeeCurrencyService?

    public init(
        host: ExtrinsicFeeEstimatorHostProtocol,
        tokenConverter: HydrationTokenConverting
    ) {
        self.host = host
        self.tokenConverter = tokenConverter
    }
}

private extension HydraExtrinsicFeeInstallerFactory {
    func createFeeServiceStateOperation(
        for _: ChainAssetProtocol,
        payerAccountId: AccountId
    ) -> BaseOperation<HydraDx.SwapFeeCurrencyState> {
        let feeService = FeeSharedStateStore.getOrCreateHydraFeeCurrencyService(
            for: host,
            payerAccountId: payerAccountId
        )

        feeCurrencyService = feeService

        return feeService.createFetchOperation()
    }

    func createMappingOperation(
        dependingOn stateOperation: BaseOperation<HydraDx.SwapFeeCurrencyState>,
        chainAsset: ChainAssetProtocol,
        tokenConverter: HydrationTokenConverting
    ) -> BaseOperation<ExtrinsicFeeInstalling> {
        ClosureOperation<ExtrinsicFeeInstalling> {
            let state = try stateOperation.extractNoCancellableResultData()

            return HydraExtrinsicFeeInstaller(
                feeAsset: chainAsset,
                swapState: state,
                tokenConverter: tokenConverter
            )
        }
    }
}

extension HydraExtrinsicFeeInstallerFactory: HydraExtrinsicFeeInstallerFactoryProtocol {
    public func createFeeInstaller(
        for chainAsset: ChainAssetProtocol,
        payerAccountId: AccountId
    ) -> CompoundOperationWrapper<ExtrinsicFeeInstalling> {
        let feeServiceStateOperation = createFeeServiceStateOperation(
            for: chainAsset,
            payerAccountId: payerAccountId
        )

        let mapOperation = createMappingOperation(
            dependingOn: feeServiceStateOperation,
            chainAsset: chainAsset,
            tokenConverter: tokenConverter
        )

        mapOperation.addDependency(feeServiceStateOperation)

        return CompoundOperationWrapper(
            targetOperation: mapOperation,
            dependencies: [feeServiceStateOperation]
        )
    }
}
