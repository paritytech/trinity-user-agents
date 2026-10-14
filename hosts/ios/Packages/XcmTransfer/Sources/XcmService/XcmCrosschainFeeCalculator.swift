import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import ChainStore
import SDKLogger

final class XcmCrosschainFeeCalculator {
    let legacyCalculator: XcmCrosschainFeeCalculating
    let dynamicCalculator: XcmCrosschainFeeCalculating

    init(
        chainRegistry: ChainResourceProtocol,
        callDerivator: XcmCallDerivating,
        tokenMintingFactory: TokenBalanceMintingFactoryProtocol,
        depositEventMatchingFactory: TokenDepositEventMatcherFactoryProtocol,
        extrinsicServiceFactory: ExtrinsicServiceFactoryProtocol,
        originDefiningFactory: ExtrinsicOriginDefiningFactoryProtocol,
        operationQueue: OperationQueue,
        wallet: MetaAccountModelProtocol,
        logger: SDKLoggerProtocol
    ) {
        legacyCalculator = XcmLegacyCrosschainFeeCalculator(
            chainRegistry: chainRegistry,
            extrinsicServiceFactory: extrinsicServiceFactory,
            originDefiningFactory: originDefiningFactory,
            operationQueue: operationQueue,
            wallet: wallet
        )

        dynamicCalculator = XcmDynamicCrosschainFeeCalculator(
            chainRegistry: chainRegistry,
            callDerivator: callDerivator,
            tokenMintingFactory: tokenMintingFactory,
            depositEventMatchingFactory: depositEventMatchingFactory,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}

extension XcmCrosschainFeeCalculator: XcmCrosschainFeeCalculating {
    func crossChainFeeWrapper(
        request: XcmUnweightedTransferRequest
    ) -> CompoundOperationWrapper<XcmFeeModelProtocol> {
        switch request.metadata.fee {
        case .legacy:
            legacyCalculator.crossChainFeeWrapper(request: request)
        case .dynamic:
            dynamicCalculator.crossChainFeeWrapper(request: request)
        }
    }
}
