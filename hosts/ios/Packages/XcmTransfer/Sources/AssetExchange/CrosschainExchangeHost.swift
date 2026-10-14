import Foundation
import SubstrateSdk
import ChainStore
import AssetExchange
import SDKLogger

public protocol CrosschainExchangeHostProtocol {
    var wallet: MetaAccountModelProtocol { get }
    var allChains: [ChainId: ChainProtocol] { get }
    var chainRegistry: ChainResourceProtocol { get }
    var xcmService: XcmTransferServiceProtocol { get }
    var resolutionFactory: XcmTransferResolutionFactoryProtocol { get }
    var xcmTransfers: XcmTransfers { get }
    var operationQueue: OperationQueue { get }
    var executionTimeEstimator: AssetExchangeTimeEstimating { get }
    var fungibilityPreservationProvider: AssetFungibilityPreservationProviding { get }
    var tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol { get }
    var balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol { get }
    var chainsWithExpensiveCrosschain: Set<ChainId> { get }
    var logger: SDKLoggerProtocol { get }
}

public final class CrosschainExchangeHost: CrosschainExchangeHostProtocol {
    public let wallet: MetaAccountModelProtocol
    public let allChains: [ChainId: ChainProtocol]
    public let chainRegistry: ChainResourceProtocol
    public let xcmService: XcmTransferServiceProtocol
    public let resolutionFactory: XcmTransferResolutionFactoryProtocol
    public let xcmTransfers: XcmTransfers
    public let executionTimeEstimator: AssetExchangeTimeEstimating
    public let fungibilityPreservationProvider: AssetFungibilityPreservationProviding
    public let tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol
    public let balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol
    public let chainsWithExpensiveCrosschain: Set<ChainId>
    public let operationQueue: OperationQueue
    public let logger: SDKLoggerProtocol

    public init(
        wallet: MetaAccountModelProtocol,
        allChains: [ChainId: ChainProtocol],
        chainRegistry: ChainResourceProtocol,
        xcmService: XcmTransferServiceProtocol,
        resolutionFactory: XcmTransferResolutionFactoryProtocol,
        xcmTransfers: XcmTransfers,
        executionTimeEstimator: AssetExchangeTimeEstimating,
        fungibilityPreservationProvider: AssetFungibilityPreservationProviding,
        tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol,
        balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol,
        chainsWithExpensiveCrosschain: Set<ChainId>,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.wallet = wallet
        self.allChains = allChains
        self.chainRegistry = chainRegistry
        self.xcmService = xcmService
        self.resolutionFactory = resolutionFactory
        self.xcmTransfers = xcmTransfers
        self.executionTimeEstimator = executionTimeEstimator
        self.fungibilityPreservationProvider = fungibilityPreservationProvider
        self.tokensDepositMatchingFactory = tokensDepositMatchingFactory
        self.balanceDetectionFactory = balanceDetectionFactory
        self.chainsWithExpensiveCrosschain = chainsWithExpensiveCrosschain
        self.operationQueue = operationQueue
        self.logger = logger
    }
}
