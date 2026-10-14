import ExternalAccessibility
import Foundation
import Foundation_iOS
import PolkadotUI
import UIKitExt

protocol BackupSyncPresentable: AnyObject {}

@MainActor
extension BackupSyncPresentable {
    func showCancelBackupConfirmation(
        from view: (any ControllerBackedProtocol)?,
        onConfirm: @escaping @MainActor () -> Void
    ) {
        let viewModel = TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in String(localized: .BalanceSync.cancelTitle) },
            message: LocalizableResource { _ in .normal(String(localized: .BalanceSync.cancelDescription)) },
            mainAction: .init(
                title: LocalizableResource { _ in String(localized: .BalanceSync.cancelConfirm) },
                handler: {
                    Task { @MainActor in
                        onConfirm()
                        view?.controller.showToast(
                            message: String(localized: .BalanceSync.backupUpdatedMessage),
                            type: .success
                        )
                    }
                },
                accessibilityId: AccessibilityID.Wallet.backupDoneConfirmButton
            ),
            secondaryAction: nil
        )
        let sheetView = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: MessageSheetStyler.balanceSync()
        )
        BottomSheetViewFacade.setupBottomSheet(from: sheetView.controller, preferredHeight: nil)
        view?.controller.present(sheetView.controller, animated: true)
    }

    func showWhyBackupUpdate(from view: (any ControllerBackedProtocol)?) {
        let viewModel = TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in String(localized: .BalanceSync.whyTitle) },
            message: LocalizableResource { _ in .normal(String(localized: .BalanceSync.whyDescription)) },
            mainAction: nil,
            secondaryAction: .init(
                title: LocalizableResource { _ in String(localized: .Common.close) },
                handler: {}
            )
        )
        let sheetView = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: MessageSheetStyler.balanceSync()
        )
        BottomSheetViewFacade.setupBottomSheet(from: sheetView.controller, preferredHeight: nil)
        view?.controller.present(sheetView.controller, animated: true)
    }
}
