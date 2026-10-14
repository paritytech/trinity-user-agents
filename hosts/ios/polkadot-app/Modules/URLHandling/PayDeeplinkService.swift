import BigInt
import Coinage
import Foundation
import SubstrateSdk
import ChainRegistry

final class PayDeeplinkService {
    private let coinageService: CoinageServicing
    private let moduleNavigator: ModuleNavigating

    init(
        coinageService: CoinageServicing,
        moduleNavigator: ModuleNavigating
    ) {
        self.coinageService = coinageService
        self.moduleNavigator = moduleNavigator
    }
}

extension PayDeeplinkService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard url.host() == "pay", url.path.isEmpty else {
            return false
        }

        guard
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            let queryItems = components.queryItems,
            let addressString = queryItems.first(where: { $0.name == "address" })?.value,
            let amountString = queryItems.first(where: { $0.name == "amount" })?.value,
            let amount = BigUInt(amountString),
            let accountId = try? addressString.toAccountId()
        else {
            return false
        }

        let lockAmount = queryItems
            .first(where: { $0.name == "lockAmount" })?.value
            .flatMap(Bool.init) ?? true

        let recipient = RecipientModel(accountId: accountId, username: nil)

        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let chainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = chainRegistry.getChain(for: chainAssetId.chainId),
            let chainAsset = chain.chainAsset(for: chainAssetId.assetId)
        else {
            return false
        }

        Task { @MainActor [coinageService, moduleNavigator] in
            guard let view = TransferAmountViewFactory.createExternalPayment(
                for: chainAsset,
                recipient: recipient,
                coinageService: coinageService,
                amountInPlanks: amount,
                lockAmount: lockAmount
            ) else {
                return
            }

            moduleNavigator.presentModally(view.controller)
        }

        return true
    }
}
