import Foundation
import SubstrateSdk
import SDKLogger

///  Protocol is designed to provide methods to create a subscription
///  for runtime version in a particular chain

public protocol SpecVersionSubscriptionFactoryProtocol: AnyObject {
    ///  Creates a subsription for runtime version in particular chain.
    ///
    ///  - Parameters:
    ///      - chain: Chain for which subscription should be created;
    ///      - connection: Connection to send request to the chain and receive updates.
    ///
    ///  - Returns: `SpecVersionSubscriptionProtocol` conforming subscription.
    func createSubscription(
        for chain: ChainModel,
        connection: JSONRPCEngine
    ) -> SpecVersionSubscriptionProtocol
}

///  Class is designed to implement `SpecVersionSubscriptionFactoryProtocol` in a way to create
///  `SpecVersionSubscription` subscription.

public final class SpecVersionSubscriptionFactory {
    public let runtimeSyncService: RuntimeSyncServiceProtocol
    public let logger: SDKLoggerProtocol?

    ///  Creates new subscription factory
    ///
    ///  - Paramaters:
    ///      - runtimeSyncService: a sync service that is shared between
    ///      subscriptions created by the factory;
    ///      - logger: logger to provide info for debugging.
    public init(runtimeSyncService: RuntimeSyncServiceProtocol, logger: SDKLoggerProtocol? = nil) {
        self.runtimeSyncService = runtimeSyncService
        self.logger = logger
    }
}

extension SpecVersionSubscriptionFactory: SpecVersionSubscriptionFactoryProtocol {
    public func createSubscription(
        for chain: ChainModel,
        connection: JSONRPCEngine
    ) -> SpecVersionSubscriptionProtocol {
        if chain.isFullSyncMode, chain.hasSubstrateRuntime {
            SpecVersionSubscription(
                chainId: chain.chainId,
                runtimeSyncService: runtimeSyncService,
                connection: connection
            )
        } else {
            NoRuntimeVersionSubscription(
                chainId: chain.chainId,
                connection: connection,
                logger: logger
            )
        }
    }
}
