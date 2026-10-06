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
    func setMediaPermission(_ setting: TrUAPIMediaPermissionSetting, allowed: Bool)
}

@MainActor
protocol AppPermissionsInteractorOutputProtocol: AnyObject {
    func didReceive(grants: [ProductPermissionGrant])
    func didReceive(mediaPermissions: [TrUAPIMediaPermissionSetting])
}

@MainActor
protocol AppPermissionsWireframeProtocol: AnyObject, AlertPresentable {}
