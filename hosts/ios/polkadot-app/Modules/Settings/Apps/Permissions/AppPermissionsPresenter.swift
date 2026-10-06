import Foundation
import PolkadotUI
import Products

@MainActor
final class AppPermissionsPresenter {
    weak var view: AppPermissionsViewProtocol?

    private let wireframe: AppPermissionsWireframeProtocol
    private let interactor: AppPermissionsInteractorInputProtocol
    private let viewModelFactory: AppPermissionsViewModelMaking

    private let productName: String

    private var grants: [(id: String, record: AppPermissionRecord)] = []
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
        guard grants.contains(where: { $0.id == item.id }) else {
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
        let records = grants.filter { pendingDeletionIds.contains($0.id) }.map(\.record)
        pendingDeletionIds.removeAll()

        interactor.revokeOnDisappear(records: records)
    }
}

extension AppPermissionsPresenter: AppPermissionsInteractorOutputProtocol {
    func didReceive(error: Error) {
        wireframe.present(
            message: String(describing: error),
            title: String(localized: .Common.error),
            closeAction: String(localized: .Common.close),
            from: view
        )
    }

    func didReceive(grants: [AppPermissionRecord]) {
        self.grants = grants.map { record in
            (self.grants.first { $0.record.hasSameIdentity(as: record) }?.id ?? UUID().uuidString, record)
        }

        let validIds = Set(self.grants.map(\.id))
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
