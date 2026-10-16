import Foundation
import PolkadotUI
import Products

import TrUAPIHost
final class AppPermissionsPresenter {
    weak var view: AppPermissionsViewProtocol?

    private let wireframe: AppPermissionsWireframeProtocol
    private let interactor: AppPermissionsInteractorInputProtocol
    private let viewModelFactory: AppPermissionsViewModelMaking

    private let productName: String

    private var grantsByItemId: [String: ProductPermissionGrant] = [:]
    private var grants: [ProductPermissionGrant] = []
    private var mediaPermissions: [TrUAPIMediaPermissionSetting] = []
    private var revoking = false

    init(
        productName: String,
        interactor: AppPermissionsInteractorInputProtocol,
        wireframe: AppPermissionsWireframeProtocol,
        viewModelFactory: AppPermissionsViewModelMaking
    ) {
        self.productName = productName
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
    }
}

extension AppPermissionsPresenter: AppPermissionsPresenterProtocol {
    func setup() {
        view?.setTitle(
            String(localized: .appPermissionsTitleFormat(productName))
        )
        interactor.setup()
    }

    func toggle(_ item: AppPermissionsViewLayout.Item, isOn: Bool) {
        if let setting = mediaPermissions.first(where: { $0.id == item.id }) {
            interactor.setMediaPermission(setting, allowed: isOn)
            return
        }
        guard !isOn, !revoking, let grant = grantsByItemId[item.id] else { return }
        revoking = true
        view?.setRevoking(true)
        // Keep the switch on until the runtime acknowledges the revoke.
        interactor.revoke(permissions: [grant.permission])
    }
}

extension AppPermissionsPresenter: AppPermissionsInteractorOutputProtocol {
    func didReceive(grants: [ProductPermissionGrant]) {
        self.grants = grants
        grantsByItemId = Dictionary(
            uniqueKeysWithValues: grants.map { ($0.identifier, $0) }
        )

        refreshItems()
    }

    func didReceive(mediaPermissions: [TrUAPIMediaPermissionSetting]) {
        self.mediaPermissions = mediaPermissions
        refreshItems()
    }

    func didFinishRevoking() {
        revoking = false
        view?.setRevoking(false)
    }

    func didReceive(error: Error) {
        wireframe.present(
            message: error.localizedDescription,
            title: String(localized: .appPermissionsTitleFormat(productName)),
            closeAction: String(localized: "OK"),
            from: view
        )
    }
}

private extension AppPermissionsPresenter {
    func refreshItems() {
        var items = viewModelFactory.createItems(from: grants)
        items.append(contentsOf: mediaPermissions.map { setting in
            let status: String
            switch setting.status {
            case .authorized: status = "Allowed"
            case .denied: status = "Denied"
            case .notDetermined: status = "Ask"
            }
            return AppPermissionsViewLayout.Item(id: setting.id, title: setting.title,
                description: "\(status)\n\(setting.detail)", isOn: setting.status == .authorized)
        })
        view?.didReceive(items: items)
    }
}
