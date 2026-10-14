import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange
import OperationExt
import ChainRegistry

final class AssetExchangePriceStore: AnyProviderAutoCleaning {
    private var store: [ChainAssetId: PriceData] = [:]
    private var priceIds: [ChainAssetId: String] = [:]
    private let mutex = NSLock()

    private var priceProvider: StreamableProvider<PriceData>?

    let priceLocalSubscriptionFactory: PriceProviderFactoryProtocol
    let chainRegistry: ChainRegistryProtocol
    let currency: Currency
    let workQueue: DispatchQueue
    let logger: LoggerProtocol

    init(
        priceLocalSubscriptionFactory: PriceProviderFactoryProtocol,
        chainRegistry: ChainRegistryProtocol,
        currency: Currency = SelectedCurrencyManager.shared.selectedCurrency,
        workQueue: DispatchQueue = DispatchQueue.global(),
        logger: LoggerProtocol
    ) {
        self.priceLocalSubscriptionFactory = priceLocalSubscriptionFactory
        self.chainRegistry = chainRegistry
        self.currency = currency
        self.workQueue = workQueue
        self.logger = logger

        setup()
    }

    deinit {
        chainRegistry.chainsUnsubscribe(self)
    }
}

private extension AssetExchangePriceStore {
    func setup() {
        chainRegistry.chainsSubscribe(
            self,
            runningInQueue: workQueue
        ) { [weak self] changes in
            guard let self else {
                return
            }

            mutex.lock()

            updatePriceSubscriptionIfNeeded(with: changes)

            mutex.unlock()
        }
    }

    func updatePriceSubscriptionIfNeeded(with changes: [DataProviderChange<ChainModel>]) {
        logger.debug("Handling chain changes: \(changes.count)")

        let previousAssetIds = Set(priceIds.keys)

        priceIds = changes.reduce(into: priceIds) { accum, change in
            switch change {
            case let .insert(item),
                 let .update(item):
                item.chainAssets().forEach { chainAsset in
                    accum[chainAsset.chainAssetId] = chainAsset.asset.priceId
                }
            case let .delete(identifier):
                accum = accum.filter { $0.key.chainId != identifier }
            }
        }

        let newAssetIds = Set(priceIds.keys)

        guard previousAssetIds != newAssetIds else {
            return
        }

        updatePriceSubscription()
    }

    func updatePriceSubscription() {
        clear(streamableProvider: &priceProvider)
        priceProvider = subscribeAllPrices(for: priceIds, currency: currency)

        if priceProvider != nil {
            logger.debug("Price subscription setup for: \(priceIds.count)")
        } else {
            logger.error("Price subscription failed")
        }
    }
}

extension AssetExchangePriceStore: AssetExchangePriceStoring {
    func getCurrencyId() -> Int? {
        currency.id
    }

    func fetchPrice(for assetId: ChainAssetId) -> AssetExchangePrice? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return store[assetId]?.decimalRate
    }
}

extension AssetExchangePriceStore: PriceLocalStorageSubscriber, PriceLocalSubscriptionHandler {
    func handleAllPrices(result: Result<[ChainAssetId: PriceData], Error>) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        switch result {
        case let .success(prices):
            logger.debug("Received prices: \(prices.count)")

            store.merge(prices) { $1 }
        case let .failure(error):
            logger.error("Unexpected error: \(error)")
        }
    }
}
