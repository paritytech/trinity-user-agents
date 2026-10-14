import Foundation
import SubstrateSdk
import ExtrinsicService
import Foundation_iOS

private struct StateKey: Hashable {
    let chainId: ChainId
    let accountId: AccountId?
}

enum FeeSharedStateStore {
    private static var states: [StateKey: WeakWrapper] = [:]
    private static var feeServices: [StateKey: WeakWrapper] = [:]
    private static let mutex = NSLock()

    static func getOrCreateHydra(
        for host: ExtrinsicFeeEstimatorHostProtocol,
        tokenConverter: HydrationTokenConverting
    ) -> HydraFlowState {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let state = StateKey(chainId: host.chain.chainId, accountId: nil)

        if let flowState = states[state]?.target as? HydraFlowState {
            return flowState
        }

        let flowState = HydraFlowState(
            chain: host.chain,
            connection: host.connection,
            runtimeProvider: host.runtimeProvider,
            tokenConverter: tokenConverter,
            operationQueue: host.operationQueue,
            logger: host.logger
        )

        states[state] = WeakWrapper(target: flowState)

        return flowState
    }

    static func getOrCreateHydraFeeCurrencyService(
        for host: ExtrinsicFeeEstimatorHostProtocol,
        payerAccountId: AccountId
    ) -> HydraSwapFeeCurrencyService {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let state = StateKey(chainId: host.chain.chainId, accountId: payerAccountId)

        if let service = feeServices[state]?.target as? HydraSwapFeeCurrencyService {
            return service
        }

        let service = HydraSwapFeeCurrencyService(
            payerAccountId: payerAccountId,
            connection: host.connection,
            runtimeProvider: host.runtimeProvider,
            operationQueue: host.operationQueue,
            logger: host.logger
        )

        feeServices[state] = WeakWrapper(target: service)

        service.setup()

        return service
    }
}
