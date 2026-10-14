import Foundation_iOS
import Products
import UIKit
import UIKitExt
import PolkadotUI

@MainActor
enum AllowancePromptViewFactory {
    static func createView(context: AllowancePromptContext) -> ControllerBackedProtocol {
        let viewModel = makeViewModel(for: context)
        let styler = ProductPromptStyler()

        let view = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: styler,
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: view.controller)

        return view
    }
}

// MARK: - ViewModel

private extension AllowancePromptViewFactory {
    static func makeViewModel(
        for context: AllowancePromptContext
    ) -> TitleDetailsSheetViewModel {
        let descriptions = context.resources.map { resourceDescription(for: $0) }
        let body = descriptions.map { "• \($0)" }.joined(separator: "\n")

        return TitleDetailsSheetViewModel(
            graphics: makeIcon(systemName: "shield.lefthalf.filled"),
            title: LocalizableResource { _ in
                String(
                    localized: .Products.allowancePromptTitle(
                        productId: context.productId
                    )
                )
            },
            message: LocalizableResource { _ in .normal(body) },
            mainAction: makeAction(
                title: String(localized: .Common.approve)
            ) { context.deliver(.approved) },
            secondaryAction: makeAction(
                title: String(localized: .Common.reject)
            ) { context.deliver(.rejected) }
        )
    }

    static func resourceDescription(for resource: AllocatableResource) -> String {
        switch resource {
        case .statementStoreAllowance:
            String(localized: .Products.allowanceResourceStatementStore)
        case .bulletInAllowance:
            String(localized: .Products.allowanceResourceBulletIn)
        case .smartContractAllowance:
            String(localized: .Products.allowanceResourceSmartContract)
        case .autoSigning:
            String(localized: .Products.allowanceResourceAutoSigning)
        }
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

    static func makeIcon(systemName: String) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: systemName, withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
