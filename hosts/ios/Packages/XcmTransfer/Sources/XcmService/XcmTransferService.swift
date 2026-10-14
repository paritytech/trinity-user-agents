import Foundation
import BigInt
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import ChainStore
import SDKLogger

public final class XcmTransferService {
    let wallet: MetaAccountModelProtocol
    let chainRegistry: ChainResourceProtocol
    let operationQueue: OperationQueue
    let extrinsicServiceFactory: ExtrinsicServiceFactoryProtocol
    let originDefiningFactory: ExtrinsicOriginDefiningFactoryProtocol
    let submissionVerifier: XcmTransferVerifying

    let callDerivator: XcmCallDerivating
    let crosschainFeeCalculator: XcmCrosschainFeeCalculating

    public init(
        wallet: MetaAccountModelProtocol,
        chainRegistry: ChainResourceProtocol,
        extrinsicServiceFactory: ExtrinsicServiceFactoryProtocol,
        originDefiningFactory: ExtrinsicOriginDefiningFactoryProtocol,
        tokenMintingFactory: TokenBalanceMintingFactoryProtocol,
        depositEventMatchingFactory: TokenDepositEventMatcherFactoryProtocol,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.wallet = wallet
        self.chainRegistry = chainRegistry
        self.extrinsicServiceFactory = extrinsicServiceFactory
        self.originDefiningFactory = originDefiningFactory
        self.operationQueue = operationQueue

        callDerivator = XcmOneOfCallDerivator(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )

        crosschainFeeCalculator = XcmCrosschainFeeCalculator(
            chainRegistry: chainRegistry,
            callDerivator: callDerivator,
            tokenMintingFactory: tokenMintingFactory,
            depositEventMatchingFactory: depositEventMatchingFactory,
            extrinsicServiceFactory: extrinsicServiceFactory,
            originDefiningFactory: originDefiningFactory,
            operationQueue: operationQueue,
            wallet: wallet,
            logger: logger
        )

        submissionVerifier = XcmTransferVerifier(
            chainRegistry: chainRegistry,
            depositEventMatchingFactory: depositEventMatchingFactory,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
