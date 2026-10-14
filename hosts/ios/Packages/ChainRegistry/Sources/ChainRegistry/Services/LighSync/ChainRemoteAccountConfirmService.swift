import Foundation
import Operation_iOS
import SubstrateSdk
import CommonService
import SDKLogger
import SubstrateSdkExt

/// For the parachains, there is an issue that leads to the returning old value if a client subscribes for
///  it right after changes. This class assures that the new state is applied onchain and notifies the main service
///  to switch the state.

public final class ChainRemoteAccountConfirmService: BaseSyncService {
    public let shouldConfirm: Bool

    public let accountId: AccountId
    public let connection: JSONRPCEngine
    public let callbackQueue: DispatchQueue
    public let detectionClosure: () -> Void

    public let storageKeyFactory = StorageKeyFactory()

    private var subscription: StorageSubscriptionContainer?
    private var isConfirming: Bool = false

    public init(
        accountId: AccountId,
        connection: JSONRPCEngine,
        shouldConfirm: Bool,
        detectionClosure: @escaping () -> Void,
        callbackQueue: DispatchQueue,
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(multiplier: 1),
        logger: SDKLoggerProtocol
    ) {
        self.accountId = accountId
        self.connection = connection
        self.detectionClosure = detectionClosure
        self.callbackQueue = callbackQueue
        self.shouldConfirm = shouldConfirm

        super.init(retryStrategy: retryStrategy, logger: logger)
    }

    private func retryConfirmation() {
        retryAttempt += 1

        subscription = nil

        retry()
    }

    private func handleHasAccount(_ hasAccount: Bool) {
        guard hasAccount else {
            if isConfirming {
                retryConfirmation()
            }

            return
        }

        if isConfirming || !shouldConfirm {
            subscription = nil
            dispatchInQueueWhenPossible(callbackQueue, block: detectionClosure)
        } else {
            isConfirming = true
            retryAttempt = 0
            retryConfirmation()
        }
    }

    override public func performSyncUp() {
        do {
            let remoteKey = try storageKeyFactory.accountInfoKeyForId(accountId)

            let handler = RawDataStorageSubscription(remoteStorageKey: remoteKey) { [weak self] data, _ in
                let hasAccount = data != nil

                self?.mutex.lock()

                defer {
                    self?.mutex.unlock()
                }

                self?.handleHasAccount(hasAccount)
            }

            subscription = StorageSubscriptionContainer(
                engine: connection,
                children: [handler],
                logger: logger
            )
        } catch {
            completeImmediate(error)
        }
    }

    override public func stopSyncUp() {
        subscription = nil
    }
}
