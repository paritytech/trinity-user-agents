import struct TrUAPIHost.ProductId
import Foundation_iOS
import PolkadotUI
import Products
import SubstrateSdk
import UIKit
import UIKitExt

@MainActor
enum TrUAPIActionPromptViewFactory {
    static func createView(context: TrUAPIActionConfirmationContext) -> ControllerBackedProtocol {
        let view = TitleDetailsSheetViewFactory.createView(
            from: makeViewModel(for: context),
            styler: ProductPromptStyler(),
            allowsSwipeDown: false
        )
        BottomSheetViewFacade.setupBottomSheet(from: view.controller)
        return view
    }

    static func makeViewModel(for context: TrUAPIActionConfirmationContext) -> TitleDetailsSheetViewModel {
        let title: String
        let body: String
        let iconName: String
        switch context.request {
        case let .productSubtree(productId):
            title = String(localized: .Products.actionProductSubtreeTitle)
            body = String(localized: .Products.actionProductSubtreeBody(productId: productId.rawValue))
            iconName = "person.crop.circle"
        case let .nftPurseDrain(productId, fromPurse, ownerProductId, itemCount, destination):
            title = String(localized: .Products.actionNftPurseDrainTitle)
            body = makeNftPurseDrainBody(
                productId: productId,
                fromPurse: fromPurse,
                owner: ownerProductId?.rawValue ?? String(localized: .Products.actionNftPurseDrainOwnerYou),
                itemCount: itemCount,
                destination: destination
            )
            iconName = "square.stack.3d.up"
        case let .coinPaymentSpend(productId, fromPurse, amount, takesHeldBackValue):
            title = String(localized: .Products.actionCoinPaymentSpendTitle)
            body = makeCoinPaymentSpendBody(
                productId: productId,
                fromPurse: fromPurse,
                amount: amount,
                takesHeldBackValue: takesHeldBackValue
            )
            iconName = "banknote"
        }

        let icon = UIImage(systemName: iconName, withConfiguration: UIImage.SymbolConfiguration(
            pointSize: 60, weight: .regular
        ))?.withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)

        return TitleDetailsSheetViewModel(
            graphics: icon,
            title: LocalizableResource { _ in title },
            message: LocalizableResource { _ in .normal(body) },
            mainAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Common.confirm) },
                handler: { context.deliver(true) }
            ),
            secondaryAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Common.reject) },
                handler: { context.deliver(false) }
            )
        )
    }

    private static func makeNftPurseDrainBody(
        productId: ProductId,
        fromPurse: UInt32,
        owner: String,
        itemCount: UInt32,
        destination: NftPurseDrainTarget
    ) -> String {
        switch destination {
        case let .purse(toPurse):
            String(localized: .Products.actionNftPurseDrainToPurseBody(
                productId: productId.rawValue,
                itemCount: String(itemCount),
                fromPurse: String(fromPurse),
                owner: owner,
                toPurse: String(toPurse)
            ))
        case let .key(hex):
            String(localized: .Products.actionNftPurseDrainToKeyBody(
                productId: productId.rawValue,
                itemCount: String(itemCount),
                fromPurse: String(fromPurse),
                owner: owner,
                key: hex
            ))
        case .cheque:
            String(localized: .Products.actionNftPurseDrainToChequeBody(
                productId: productId.rawValue,
                itemCount: String(itemCount),
                fromPurse: String(fromPurse),
                owner: owner
            ))
        case .refund:
            String(localized: .Products.actionNftPurseDrainToRefundBody(
                productId: productId.rawValue,
                itemCount: String(itemCount),
                fromPurse: String(fromPurse),
                owner: owner
            ))
        }
    }

    private static func makeCoinPaymentSpendBody(
        productId: ProductId,
        fromPurse: UInt32,
        amount: Balance,
        takesHeldBackValue: Bool
    ) -> String {
        let formattedAmount = PaymentRequestViewFactory.mainChainAsset().map {
            PaymentRequestViewFactory.formatAmount(amount, chainAsset: $0)
        } ?? String(amount)
        let spend = String(localized: .Products.actionCoinPaymentSpendBody(
            productId: productId.rawValue,
            amount: formattedAmount,
            fromPurse: CoinPaymentPurseCopy.name(purse: fromPurse)
        ))
        guard takesHeldBackValue else {
            return spend
        }
        return spend + "\n\n" + String(localized: .Products.actionCoinPaymentSpendHeldBackValue)
    }
}
