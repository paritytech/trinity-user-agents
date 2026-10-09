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
        let items = viewModelFactory.createItems(from: grants)
        view?.didReceive(items: items)
    }
}
