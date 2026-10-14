import Foundation
import SubstrateSdk
import SDKLogger
import SubstrateSdkExt

public protocol SpecVersionSubscriptionProtocol: AnyObject {
    func subscribe()
    func unsubscribe()
}

public final class SpecVersionSubscription {
    public let chainId: ChainModel.Id
    public let runtimeSyncService: RuntimeSyncServiceProtocol
    public let connection: JSONRPCEngine
    public let logger: SDKLoggerProtocol?

    private(set) var subscriptionId: UInt16?

    public init(
        chainId: ChainModel.Id,
        runtimeSyncService: RuntimeSyncServiceProtocol,
        connection: JSONRPCEngine,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.chainId = chainId
        self.runtimeSyncService = runtimeSyncService
        self.connection = connection
        self.logger = logger
    }
}

extension SpecVersionSubscription: SpecVersionSubscriptionProtocol {
    public func subscribe() {
        do {
            let updateClosure: (RuntimeVersionUpdate) -> Void = { [weak self] update in
                guard let strongSelf = self else {
                    return
                }

                let runtimeVersion = update.params.result
                strongSelf.logger?.debug("For chain: \(strongSelf.chainId)")
                strongSelf.logger?.debug("Did receive spec version: \(runtimeVersion.specVersion)")
                strongSelf.logger?.debug("Did receive tx version: \(runtimeVersion.transactionVersion)")

                strongSelf.runtimeSyncService.apply(
                    version: runtimeVersion,
                    for: strongSelf.chainId
                )
            }

            let failureClosure: (Error, Bool) -> Void = { [weak self] error, unsubscribed in
                self?.logger?.error("Unexpected failure after subscription: \(error) \(unsubscribed)")
            }

            let params: [String] = []
            subscriptionId = try connection.subscribe(
                RPCMethod.runtimeVersionSubscribe,
                params: params,
                updateClosure: updateClosure,
                failureClosure: failureClosure
            )
        } catch {
            logger?.error("Unexpected chain \(chainId) subscription failure: \(error)")
        }
    }

    public func unsubscribe() {
        if let identifier = subscriptionId {
            subscriptionId = nil
            connection.cancelForIdentifier(identifier)
        }
    }
}

public final class NoRuntimeVersionSubscription: SpecVersionSubscriptionProtocol {
    public let chainId: ChainModel.Id
    public let connection: JSONRPCEngine
    public let logger: SDKLoggerProtocol?

    public init(chainId: ChainModel.Id, connection: JSONRPCEngine, logger: SDKLoggerProtocol?) {
        self.chainId = chainId
        self.connection = connection
        self.logger = logger
    }

    public func subscribe() {
        logger?.warning("We do nothing but holding shared connection this can be changed in future")
    }

    public func unsubscribe() {
        logger?.warning("We do nothing but holding shared connection this can be changed in future")
    }
}
