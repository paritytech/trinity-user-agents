import Foundation
import PolkadotUI
import Products
import SubstrateSdk

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
    private var pendingDeletionIds: Set<String> = []
    private var automaticUploadScope: TrUAPIAutomaticUploadScope?
    private var automaticUploadsAllowed = false

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
        if let scope = automaticUploadScope, item.id == automaticUploadItemId(scope) {
            interactor.setAutomaticUploads(allowed: isOn, scope: scope)
            return
        }
        guard grantsByItemId[item.id] != nil else {
            return
        }

        if isOn {
            pendingDeletionIds.remove(item.id)
        } else {
            pendingDeletionIds.insert(item.id)
        }

        refreshItems()
    }

    func viewWillDisappear() {
        let permissionsToRevoke = pendingDeletionIds.compactMap { grantsByItemId[$0]?.permission }
        pendingDeletionIds.removeAll()

        interactor.revokeOnDisappear(permissions: permissionsToRevoke)
    }
}

extension AppPermissionsPresenter: AppPermissionsInteractorOutputProtocol {
    func didReceiveAutomaticUploads(scope: TrUAPIAutomaticUploadScope?, allowed: Bool) {
        automaticUploadScope = scope
        automaticUploadsAllowed = allowed
        refreshItems()
    }

    func didReceive(grants: [ProductPermissionGrant]) {
        self.grants = grants
        grantsByItemId = Dictionary(
            uniqueKeysWithValues: grants.map { ($0.identifier, $0) }
        )

        let validIds = Set(grantsByItemId.keys)
        pendingDeletionIds = pendingDeletionIds.intersection(validIds)

        refreshItems()
    }

    func didReceive(mediaPermissions: [TrUAPIMediaPermissionSetting]) {
        self.mediaPermissions = mediaPermissions
        refreshItems()
    }
}

private extension AppPermissionsPresenter {
    func automaticUploadItemId(_ scope: TrUAPIAutomaticUploadScope) -> String {
        "automatic-preimage:\(scope.generation):\(scope.rootPublicKey.toHex(includePrefix: true))"
    }

    func refreshItems() {
        var items = viewModelFactory.createItems(
            from: grants,
            pendingDeletionIds: pendingDeletionIds
        )
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
        if let scope = automaticUploadScope {
            items.append(AppPermissionsViewLayout.Item(
                id: automaticUploadItemId(scope),
                title: String(localized: .Products.appPermissionAutomaticPreimageTitle),
                description: String(localized: .Products.appPermissionAutomaticPreimageBody(
                    rootAccount: scope.rootPublicKey.toHex(includePrefix: true),
                    bulletinNetwork: scope.bulletinGenesis.toHex(includePrefix: true)
                )),
                isOn: automaticUploadsAllowed
            ))
        }
        view?.didReceive(items: items)
    }
}
