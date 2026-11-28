import Foundation
import Products
import ChainRegistry

enum PaymentApprovalRequesterFactory {
    static func create(
        router: ProductsRouting,
        paymentsSupport: PaymentsSupport?,
        chainRegistry: ChainRegistryProtocol
    ) -> PaymentApprovalRequesting {
        let mainAssetId = AppConfig.Assets.mainAsset
        let mainAsset = chainRegistry.getChain(for: mainAssetId.chainId)?.asset(for: mainAssetId.assetId)
        // Without the main asset the limit is zero, so every payment asks.
        let rule = mainAsset.map { ConfirmationGateRule(asset: $0) }
            ?? ConfirmationGateRule(absoluteLimit: 0, shareOfTotal: AppConfig.ConfirmationLimit.shareOfTotal)
        let gate = PaymentConfirmationGate(
            coinageService: paymentsSupport?.coinageService,
            rule: rule
        )

        return AuthenticatingPaymentApprovalRequester(
            gate: gate,
            authorizeDeviceOwner: { await authorizeDeviceOwner() },
            wrapped: PaymentApprovalRequester(router: router)
        )
    }
}

private extension PaymentApprovalRequesterFactory {
    @MainActor
    static func authorizeDeviceOwner() async -> Bool {
        await withCheckedContinuation { continuation in
            let authorizer = DeviceOwnerAuthorizer()
            authorizer.authorize(animated: true, retriable: false) { isAuthorized in
                continuation.resume(returning: isAuthorized)
            }
        }
    }
}
