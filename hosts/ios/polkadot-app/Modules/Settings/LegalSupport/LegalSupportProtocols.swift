import Foundation
import PolkadotUI
import UIKitExt

protocol LegalSupportViewProtocol: ControllerBackedProtocol {
    func applyContent(_ sections: [SettingsViewLayout.Section])
}

@MainActor
protocol LegalSupportPresenterProtocol: AnyObject {
    func setup()
    func didTapCell(_ cell: LegalSupportViewModel.CellType)
}

@MainActor
protocol LegalSupportWireframeProtocol: AnyObject, WebPresentable, AlertPresentable {
    func openMailComposer(from view: LegalSupportViewProtocol?)
    func showContactEmailFallback(_ email: String, from view: LegalSupportViewProtocol?)
}
