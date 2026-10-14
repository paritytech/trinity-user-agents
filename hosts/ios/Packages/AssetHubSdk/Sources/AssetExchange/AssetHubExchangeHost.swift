import Foundation
import SubstrateSdk
import ExtrinsicService
import AssetExchange
import SDKLogger

public protocol AssetHubExchangeHostProtocol {
    var chain: ChainProtocol { get }
    var selectedAccount: AccountProtocol { get }
    var submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol { get }
    var extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol { get }
    var originDefiner: ExtrinsicOriginDefining { get }
    var runtimeService: RuntimeCodingServiceProtocol { get }
    var connection: JSONRPCEngine { get }
    var operationQueue: OperationQueue { get }
    var executionTimeEstimator: AssetExchangeTimeEstimating { get }
    var flowState: AssetHubFlowStateProtocol { get }
    var extrinsicConverting: AssetHubExtrinsicConverting { get }
    var logger: SDKLoggerProtocol { get }
}

public final class AssetHubExchangeHost: AssetHubExchangeHostProtocol {
    public let chain: ChainProtocol
    public let selectedAccount: AccountProtocol
    public let flowState: AssetHubFlowStateProtocol
    public let submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol
    public let extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol
    public let originDefiner: ExtrinsicOriginDefining
    public let runtimeService: RuntimeCodingServiceProtocol
    public let connection: JSONRPCEngine
    public let executionTimeEstimator: AssetExchangeTimeEstimating
    public let extrinsicConverting: AssetHubExtrinsicConverting
    public let operationQueue: OperationQueue
    public let logger: SDKLoggerProtocol

    public init(
        chain: ChainProtocol,
        selectedAccount: AccountProtocol,
        flowState: AssetHubFlowStateProtocol,
        submissionMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol,
        extrinsicOperationFactory: ExtrinsicOperationFactoryProtocol,
        originDefiner: ExtrinsicOriginDefining,
        runtimeService: RuntimeCodingServiceProtocol,
        connection: JSONRPCEngine,
        executionTimeEstimator: AssetExchangeTimeEstimating,
        extrinsicConverting: AssetHubExtrinsicConverting,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.selectedAccount = selectedAccount
        self.flowState = flowState
        self.submissionMonitorFactory = submissionMonitorFactory
        self.extrinsicOperationFactory = extrinsicOperationFactory
        self.originDefiner = originDefiner
        self.runtimeService = runtimeService
        self.connection = connection
        self.executionTimeEstimator = executionTimeEstimator
        self.extrinsicConverting = extrinsicConverting
        self.operationQueue = operationQueue
        self.logger = logger
    }
}
