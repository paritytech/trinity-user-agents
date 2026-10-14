import Foundation
import Coinage
import Operation_iOS
import SDKLogger
import StructuredConcurrency

protocol W3sDsfinvkRouting: AnyObject {
    @MainActor
    func route(_ receipt: W3sDsfinvkReceipt) async
}

final class W3sDsfinvkRouter: W3sDsfinvkRouting {
    private let remoteConfig: RemoteConfigManaging
    private let launcher: any W3sPayLaunching
    private let logger: SDKLoggerProtocol?

    init(
        remoteConfig: RemoteConfigManaging,
        launcher: any W3sPayLaunching,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.remoteConfig = remoteConfig
        self.launcher = launcher
        self.logger = logger
    }

    @MainActor
    func route(_ receipt: W3sDsfinvkReceipt) async {
        do {
            let wrapper: CompoundOperationWrapper<[String: W3sMerchant]> =
                remoteConfig.asyncWaitW3sMerchants()
            let merchants = try await wrapper.asyncExecute()
            guard let merchant = merchants[receipt.serial] else {
                logger?.debug("W3S DSFinV-K: no merchant for serial \(receipt.serial)")
                return
            }
            launcher.launch(
                merchantKey: merchant.key,
                topic: merchant.topic,
                paymentId: receipt.paymentId,
                amount: receipt.amount,
                // Friendly name when configured; cash-register serial as fallback.
                recipientLabel: merchant.name ?? receipt.serial
            )
        } catch {
            logger?.error("W3S DSFinV-K: failed to load merchants config: \(error)")
        }
    }
}

extension W3sDsfinvkRouter {
    static func createDefault(coinageService: CoinageServicing) -> W3sDsfinvkRouting {
        let logger = Logger.shared
        let historyStorage = W3sPaymentHistoryCoreDataStore(storageFacade: UserDataStorageFacade.shared)

        let launcher = W3sPayLauncher(
            coinageService: coinageService,
            historyStore: historyStorage,
            logger: logger
        )

        return W3sDsfinvkRouter(
            remoteConfig: FirebaseFacade.shared,
            launcher: launcher,
            logger: logger
        )
    }
}
