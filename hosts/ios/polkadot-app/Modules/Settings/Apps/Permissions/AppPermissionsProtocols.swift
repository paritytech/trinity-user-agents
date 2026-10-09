import Foundation
import PolkadotUI
import Products
import UIKitExt

protocol AppPermissionsViewProtocol: ControllerBackedProtocol {
    func didReceive(items: [AppPermissionsViewLayout.Item])
    func setTitle(_ title: String)
    func setRevoking(_ revoking: Bool)
}

@MainActor
protocol AppPermissionsPresenterProtocol: AnyObject {
    func setup()
    func toggle(_ item: AppPermissionsViewLayout.Item, isOn: Bool)
}

@MainActor
protocol AppPermissionsInteractorInputProtocol: AnyObject {
    func setup()
    func setMediaPermission(_ setting: TrUAPIMediaPermissionSetting, allowed: Bool)
    func setAutomaticUploads(allowed: Bool, scope: TrUAPIAutomaticUploadScope)
    func revoke(permissions: [ProductPermission])
}

@MainActor
protocol AppPermissionsInteractorOutputProtocol: AnyObject {
    func didReceive(grants: [ProductPermissionGrant])
    func didReceive(mediaPermissions: [TrUAPIMediaPermissionSetting])
    func didReceiveAutomaticUploads(scope: TrUAPIAutomaticUploadScope?, allowed: Bool)
    func didFinishRevoking()
    func didReceive(error: Error)
}

@MainActor
protocol AppPermissionsWireframeProtocol: AnyObject, AlertPresentable {}
