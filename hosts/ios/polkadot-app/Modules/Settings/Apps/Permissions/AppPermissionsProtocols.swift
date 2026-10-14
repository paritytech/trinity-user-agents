import Foundation
import PolkadotUI
import Products
import UIKitExt

protocol AppPermissionsViewProtocol: ControllerBackedProtocol {
    func didReceive(items: [AppPermissionsViewLayout.Item])
    func setTitle(_ title: String)
}

@MainActor
protocol AppPermissionsPresenterProtocol: AnyObject {
    func setup()
    func toggle(_ item: AppPermissionsViewLayout.Item, isOn: Bool)
    func viewWillDisappear()
}

protocol AppPermissionsInteractorInputProtocol: AnyObject {
    func setup()
    func revokeOnDisappear(permissions: [ProductPermission])
}

@MainActor
protocol AppPermissionsInteractorOutputProtocol: AnyObject {
    func didReceive(grants: [ProductPermissionGrant])
}

@MainActor
protocol AppPermissionsWireframeProtocol: AnyObject, AlertPresentable {}
