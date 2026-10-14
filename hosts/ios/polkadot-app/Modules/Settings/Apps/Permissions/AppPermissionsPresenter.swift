import Foundation
import PolkadotUI
import Products

final class AppPermissionsPresenter {
    weak var view: AppPermissionsViewProtocol?

    private let wireframe: AppPermissionsWireframeProtocol
    private let interactor: AppPermissionsInteractorInputProtocol
    private let viewModelFactory: AppPermissionsViewModelMaking

    private let productName: String

    private var grantsByItemId: [String: ProductPermissionGrant] = [:]
    private var grants: [ProductPermissionGrant] = []
    private var pendingDeletionIds: Set<String> = []

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
    func didReceive(grants: [ProductPermissionGrant]) {
        self.grants = grants
        grantsByItemId = Dictionary(
            uniqueKeysWithValues: grants.map { ($0.identifier, $0) }
        )

        let validIds = Set(grantsByItemId.keys)
        pendingDeletionIds = pendingDeletionIds.intersection(validIds)

        refreshItems()
    }
}

private extension AppPermissionsPresenter {
    func refreshItems() {
        let items = viewModelFactory.createItems(
            from: grants,
            pendingDeletionIds: pendingDeletionIds
        )
        view?.didReceive(items: items)
    }
}
