import Foundation
import Operation_iOS
import ExtrinsicService
import SubstrateSdk
import SubstrateStorageQuery
import ChainStore
import SDKLogger

protocol XcmDepositMonitoringServiceProtocol {
    func useMonitoringWrapper() -> CompoundOperationWrapper<Balance>
}

enum XcmDepositMonitoringServiceError: Error {
    case unsupportedAsset(ChainAssetProtocol)
    case timeout
    case throttled
}

final class XcmDepositMonitoringService {
    let chainRegistry: ChainResourceProtocol
    let operationQueue: OperationQueue
    let workingQueue: DispatchQueue
    let logger: SDKLoggerProtocol
    let blockEventsQueryFactory: BlockEventsQueryFactoryProtocol
    let tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol
    let balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol

    let accountId: AccountId
    let chainAsset: ChainAssetProtocol
    let timeout: TimeInterval

    private var subscription: BalanceChangeDetecting?

    private var state: TokenDepositEvent?
    private var notificationClosure: ((Result<TokenDepositEvent, Error>) -> Void)?
    private var scheduler: SchedulerProtocol?
    private var detectionCallsStore: [Data: CancellableCallStore] = [:]
    private let mutex = NSLock()

    init(
        accountId: AccountId,
        chainAsset: ChainAssetProtocol,
        timeout: TimeInterval = 90,
        chainRegistry: ChainResourceProtocol,
        tokensDepositMatchingFactory: TokenDepositEventMatcherFactoryProtocol,
        balanceDetectionFactory: BalanceChangeDetectorFactoryProtocol,
        operationQueue: OperationQueue,
        workingQueue: DispatchQueue,
        logger: SDKLoggerProtocol
    ) {
        self.accountId = accountId
        self.chainAsset = chainAsset
        self.timeout = timeout
        self.chainRegistry = chainRegistry
        self.tokensDepositMatchingFactory = tokensDepositMatchingFactory
        self.balanceDetectionFactory = balanceDetectionFactory
        self.operationQueue = operationQueue
        self.workingQueue = workingQueue
        self.logger = logger

        blockEventsQueryFactory = BlockEventsQueryFactory(
            operationQueue: operationQueue,
            storageRequestFactory: StorageRequestFactory(
                remoteFactory: StorageKeyFactory(),
                operationManager: OperationManager(operationQueue: operationQueue)
            ),
            logger: logger
        )
    }

    private func notifyAboutStateIfNeeded() {
        if let state {
            let closureToNotify = notificationClosure
            notificationClosure = nil

            workingQueue.async {
                closureToNotify?(.success(state))
            }
        }
    }

    private func notifyTimeout() {
        let closureToNotify = notificationClosure
        notificationClosure = nil

        workingQueue.async {
            closureToNotify?(.failure(XcmDepositMonitoringServiceError.timeout))
        }
    }

    private func notifyCancelled() {
        guard notificationClosure != nil else {
            return
        }

        let closureToNotify = notificationClosure
        notificationClosure = nil

        workingQueue.async {
            closureToNotify?(.failure(XcmDepositMonitoringServiceError.throttled))
        }
    }

    private func clearTimeoutScheduler() {
        scheduler?.cancel()
        scheduler = nil
    }

    private func setupTimeoutScheduler() {
        scheduler = Scheduler(with: self, callbackQueue: workingQueue)
        scheduler?.notifyAfter(timeout)
    }

    private func createCodingFactoryOperation() -> BaseOperation<RuntimeCoderFactoryProtocol> {
        do {
            let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(
                for: chainAsset.chainInterface.chainId
            )

            return runtimeProvider.fetchCoderFactoryOperation()
        } catch {
            return .createWithError(error)
        }
    }

    private func createBlockDetailsWrapper(for hash: Data) -> CompoundOperationWrapper<SubstrateBlockDetails> {
        do {
            let chainId = chainAsset.chainInterface.chainId
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

            return blockEventsQueryFactory.queryBlockDetailsWrapper(
                from: connection,
                runtimeProvider: runtimeProvider,
                blockHash: hash
            )
        } catch {
            return .createWithError(error)
        }
    }

    private func fetchBlockAndDetectDeposit(
        for hash: Data,
        accountId: AccountId,
        tokensDetector: XcmTokensArrivalDetecting
    ) {
        let codingFactoryOperation = createCodingFactoryOperation()

        let blockDetailsWrapper = createBlockDetailsWrapper(for: hash)

        let matchingOperation = ClosureOperation<TokenDepositEvent?> {
            let blockDetails = try blockDetailsWrapper.targetOperation.extractNoCancellableResultData()
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            return tokensDetector.searchForXcmArrival(
                in: blockDetails,
                accountId: accountId,
                codingFactory: codingFactory
            )
        }

        matchingOperation.addDependency(codingFactoryOperation)
        matchingOperation.addDependency(blockDetailsWrapper.targetOperation)

        let totalWrapper = blockDetailsWrapper
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: matchingOperation)

        let callStore = CancellableCallStore()
        detectionCallsStore[hash] = callStore

        executeCancellable(
            wrapper: totalWrapper,
            inOperationQueue: operationQueue,
            backingCallIn: callStore,
            runningCallbackIn: workingQueue,
            mutex: mutex
        ) { [weak self] result in
            guard let self else {
                return
            }

            detectionCallsStore[hash] = nil

            switch result {
            case let .success(deposit):
                if let deposit {
                    logger.debug("Received deposit")
                    state = deposit
                    notifyAboutStateIfNeeded()
                } else {
                    logger.debug("No deposit in the block")
                }
            case let .failure(error):
                logger.debug("Block processing failed: \(error)")
            }
        }
    }

    // MARK: Protected interface

    private func setupNotificationClosure(_ closure: @escaping (Result<TokenDepositEvent, Error>) -> Void) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let state {
            workingQueue.async {
                closure(.success(state))
            }
            return
        }

        if subscription == nil {
            workingQueue.async {
                closure(.failure(XcmDepositMonitoringServiceError.unsupportedAsset(self.chainAsset)))
            }
            return
        }

        notificationClosure = closure

        setupTimeoutScheduler()
    }

    private func setupIfNeeded() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard subscription == nil else {
            return
        }

        guard
            let eventMatchers = tokensDepositMatchingFactory.createMatcher(
                for: chainAsset
            ) else {
            logger.error("Unsupported asset: \(chainAsset.assetInterface.symbol)")
            return
        }

        let tokensDetector = XcmTokensArrivalDetector(
            eventMatchers: eventMatchers,
            logger: logger
        )

        subscription = balanceDetectionFactory.createDetector(
            for: accountId,
            chainAsset: chainAsset
        )

        subscription?.subscribe(notifyingIn: workingQueue) { [weak self] result in
            guard let self else {
                return
            }

            mutex.lock()

            defer {
                mutex.unlock()
            }

            switch result {
            case let .success(blockHash):
                let chainName = chainAsset.chainInterface.name
                logger.debug("\(accountId.toHex()) Checking block \(blockHash.toHex()) in \(chainName)")

                fetchBlockAndDetectDeposit(
                    for: blockHash,
                    accountId: accountId,
                    tokensDetector: tokensDetector
                )
            case let .failure(error):
                logger.error("Remote subscription failed: \(error)")
            }
        }
    }

    private func throttle() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard subscription != nil else {
            return
        }

        subscription?.unsubscribe()
        subscription = nil

        detectionCallsStore.values.forEach { $0.cancel() }
        detectionCallsStore = [:]

        notifyCancelled()
    }
}

extension XcmDepositMonitoringService: SchedulerDelegate {
    func didTrigger(scheduler _: SchedulerProtocol) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        notifyTimeout()

        scheduler = nil
    }
}

extension XcmDepositMonitoringService: XcmDepositMonitoringServiceProtocol {
    func useMonitoringWrapper() -> CompoundOperationWrapper<Balance> {
        setupIfNeeded()

        let operation = AsyncClosureOperation(
            operationClosure: { completion in
                self.setupNotificationClosure { [weak self] result in
                    self?.throttle()

                    switch result {
                    case let .success(deposit):
                        completion(.success(deposit.amount))
                    case let .failure(error):
                        completion(.failure(error))
                    }
                }

            }, cancelationClosure: { [weak self] in
                self?.throttle()
            }
        )

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
