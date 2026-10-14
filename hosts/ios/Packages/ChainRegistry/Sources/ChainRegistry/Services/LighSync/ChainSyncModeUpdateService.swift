import Foundation
import SubstrateSdk
import Operation_iOS
import CommonService
import SDKLogger

public protocol ChainSyncModeUpdateServiceProtocol: ApplicationServiceProtocol {
    func update(selectedMetaAccount: MetaAccountModelProtocol)
}

public final class ChainSyncModeUpdateService {
    public let chainRegistry: ChainRegistryProtocol
    public let logger: SDKLoggerProtocol
    public let accountDetector: ChainRemoteAccountDetecting
    public let workingQueue = DispatchQueue.global(qos: .userInitiated)

    private var selectedMetaAccount: MetaAccountModelProtocol?
    private var mutex = NSLock()

    public init(
        selectedMetaAccount: MetaAccountModelProtocol?,
        chainRegistry: ChainRegistryProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.selectedMetaAccount = selectedMetaAccount
        self.chainRegistry = chainRegistry
        self.logger = logger

        accountDetector = SubstrateRemoteAccountDetector(logger: logger)
    }

    private func handle(changes: [DataProviderChange<ChainModel>]) {
        changes.forEach { change in
            switch change {
            case let .insert(chain),
                 let .update(chain):
                // manage runtime sync only for substrate networks
                guard !chain.noSubstrateRuntime, chain.isLightSyncMode else {
                    accountDetector.stopTrackingAccount(for: chain.chainId)
                    return
                }

                guard
                    let selectedMetaAccount,
                    let accountId = try? selectedMetaAccount.fetchAccount(for: chain).accountId else {
                    logger.debug("No account to track for \(chain.name) \(chain.chainId)")
                    return
                }

                guard let connection = chainRegistry.getConnection(for: chain.chainId) else {
                    logger.error("No connection for \(chain.name) \(chain.chainId)")
                    return
                }

                do {
                    try accountDetector.startTracking(
                        accountId: accountId,
                        chain: chain,
                        connection: connection
                    )

                    logger.debug("Started tracking account for \(chain.name)")
                } catch {
                    logger.error("Can't track account for \(chain.name) \(error)")
                }
            case let .delete(deletedIdentifier):
                accountDetector.stopTrackingAccount(for: deletedIdentifier)
            }
        }
    }

    private func subscribeChains() {
        chainRegistry.chainsSubscribe(self, runningInQueue: workingQueue) { [weak self] changes in
            self?.mutex.lock()

            self?.handle(changes: changes)

            self?.mutex.unlock()
        }
    }

    private func unsubscribeChains() {
        chainRegistry.chainsUnsubscribe(self)
    }
}

extension ChainSyncModeUpdateService: ChainRemoteAccountDetectorDelegate {
    public func didDetectAccount(for accountId: AccountId, chain: ChainModel) {
        guard
            let selectedMetaAccount,
            let currentAccountId = try? selectedMetaAccount.fetchAccount(for: chain).accountId,
            currentAccountId == accountId else {
            return
        }

        do {
            try chainRegistry.switchSync(mode: .full, chainId: chain.chainId)

            logger.debug("Switch to full mode for \(chain.name)")
        } catch {
            logger.error("Can't switch full sync \(chain.name) \(error)")
        }
    }
}

extension ChainSyncModeUpdateService: ChainSyncModeUpdateServiceProtocol {
    public func setup() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        accountDetector.delegate = self
        accountDetector.callbackQueue = .global(qos: .userInitiated)

        subscribeChains()
    }

    public func throttle() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        unsubscribeChains()
        accountDetector.stopTrackingAll()
    }

    public func update(selectedMetaAccount: MetaAccountModelProtocol) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        self.selectedMetaAccount = selectedMetaAccount

        unsubscribeChains()
        accountDetector.stopTrackingAll()

        subscribeChains()
    }
}
