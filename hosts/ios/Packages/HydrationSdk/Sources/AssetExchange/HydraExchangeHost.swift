import Foundation
import SubstrateSdk
import ExtrinsicService
import AssetExchange
import SDKLogger

public protocol HydraExchangeHostProtocol {
    var chain: ChainProtocol { get }
    var selectedAccount: AccountProtocol { get }
    var submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol { get }
    var extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol { get }
    var extrinsicParamsFactory: HydraExchangeExtrinsicParamsFactoryProtocol { get }
    var originDefiner: ExtrinsicOriginDefining { get }
    var runtimeService: RuntimeCodingServiceProtocol { get }
    var connection: JSONRPCEngine { get }
    var executionTimeEstimator: AssetExchangeTimeEstimating { get }
    var tokenConverting: HydrationTokenConverting { get }
    var operationQueue: OperationQueue { get }
    var logger: SDKLoggerProtocol { get }
}

public final class HydraExchangeHost: HydraExchangeHostProtocol {
    public let chain: ChainProtocol
    public let selectedAccount: AccountProtocol

    public let submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol
    public let extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol
    public let extrinsicParamsFactory: HydraExchangeExtrinsicParamsFactoryProtocol
    public let executionTimeEstimator: AssetExchangeTimeEstimating
    public let runtimeService: RuntimeCodingServiceProtocol
    public let connection: JSONRPCEngine
    public let originDefiner: ExtrinsicOriginDefining
    public let tokenConverting: HydrationTokenConverting
    public let operationQueue: OperationQueue
    public let logger: SDKLoggerProtocol

    public convenience init(
        chain: ChainProtocol,
        selectedAccount: AccountProtocol,
        submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol,
        extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol,
        referralCode: String?,
        runtimeService: RuntimeCodingServiceProtocol,
        connection: JSONRPCEngine,
        originDefiner: ExtrinsicOriginDefining,
        executionTimeEstimator: AssetExchangeTimeEstimating,
        tokenConverting: HydrationTokenConverting,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        let swapParamsService = HydraSwapParamsService(
            accountId: selectedAccount.accountId,
            connection: connection,
            runtimeProvider: runtimeService,
            operationQueue: operationQueue,
            logger: logger
        )

        swapParamsService.setup()

        let extrinsicParamsFactory = HydraExchangeExtrinsicParamsFactory(
            chain: chain,
            swapService: swapParamsService,
            runtimeProvider: runtimeService,
            tokenConverter: tokenConverting,
            referralCode: referralCode
        )

        self.init(
            chain: chain,
            selectedAccount: selectedAccount,
            submissionMonitorFactory: submissionMonitorFactory,
            extrinsicOperationFactory: extrinsicOperationFactory,
            extrinsicParamsFactory: extrinsicParamsFactory,
            runtimeService: runtimeService,
            connection: connection,
            originDefiner: originDefiner,
            executionTimeEstimator: executionTimeEstimator,
            tokenConverting: tokenConverting,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    public init(
        chain: ChainProtocol,
        selectedAccount: AccountProtocol,
        submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol,
        extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol,
        extrinsicParamsFactory: HydraExchangeExtrinsicParamsFactoryProtocol,
        runtimeService: RuntimeCodingServiceProtocol,
        connection: JSONRPCEngine,
        originDefiner: ExtrinsicOriginDefining,
        executionTimeEstimator: AssetExchangeTimeEstimating,
        tokenConverting: HydrationTokenConverting,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.selectedAccount = selectedAccount
        self.submissionMonitorFactory = submissionMonitorFactory
        self.extrinsicOperationFactory = extrinsicOperationFactory
        self.extrinsicParamsFactory = extrinsicParamsFactory
        self.runtimeService = runtimeService
        self.connection = connection
        self.originDefiner = originDefiner
        self.executionTimeEstimator = executionTimeEstimator
        self.tokenConverting = tokenConverting
        self.operationQueue = operationQueue
        self.logger = logger
    }
}
