import Foundation
import PolkadotUI
import Products
import SubstrateSdk
import UIKit
import UIKitExt
import Foundation_iOS
import ChainRegistry

@MainActor
enum PaymentRequestViewFactory {
    static func createView(context: PaymentRequestContext) -> ControllerBackedProtocol? {
        guard let chainAsset = mainChainAsset() else {
            return nil
        }

        let viewModel = makeViewModel(context: context, chainAsset: chainAsset)
        let styler = PaymentRequestStyler()

        let view = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: styler,
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: view.controller)

        return view
    }
}

// MARK: - Shared Helpers

extension PaymentRequestViewFactory {
    static func mainChainAsset() -> ChainAsset? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let chainAssetId = AppConfig.Assets.mainAsset

        guard let chain = chainRegistry.getChain(for: chainAssetId.chainId) else {
            return nil
        }

        return chain.chainAsset(for: chainAssetId.assetId)
    }

    static func formatAmount(_ balance: Balance, chainAsset: ChainAsset) -> String {
        let decimalAmount = balance.decimal(assetInfo: chainAsset.assetDisplayInfo)

        let formatter = AssetBalanceFormatterFactory()
            .createTokenFormatter(for: chainAsset.assetDisplayInfo)
            .value(for: .current)

        return formatter.stringFromDecimal(decimalAmount) ?? ""
    }
}

// MARK: - ViewModel

private extension PaymentRequestViewFactory {
    static func makeViewModel(
        context: PaymentRequestContext,
        chainAsset: ChainAsset
    ) -> TitleDetailsSheetViewModel {
        let formattedAmount = formatAmount(
            context.amountInPlanks,
            chainAsset: chainAsset
        )

        return TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in
                String(
                    localized: .Products.paymentRequestTitle(
                        productId: context.productId
                    )
                )
            },
            message: LocalizableResource { _ in .normal(formattedAmount) },
            mainAction: makeAction(
                title: String(localized: .Common.approve)
            ) { context.deliverApproved() },
            secondaryAction: makeAction(
                title: String(localized: .Common.reject)
            ) { context.deliverRejected() }
        )
    }

    static func makeAction(
        title: String,
        handler: @escaping () -> Void
    ) -> MessageSheetAction {
        MessageSheetAction(
            title: LocalizableResource { _ in title },
            handler: handler
        )
    }
}

// MARK: - Styler

/// Extends ``ProductPromptStyler`` with a larger details font
/// so the payment amount is prominently displayed.
private final class PaymentRequestStyler: ProductPromptStyler {
    override func applyStyle(to view: MessageSheetStyleAcceptable) {
        super.applyStyle(to: view)

        view.detailsLabel.textColor = .fgPrimary
        view.detailsLabel.font = .semibold56
        view.detailsLabel.textAlignment = .center
        view.afterTitleSpacing = 40
        view.afterDetailsSpacing = 40
    }
}
