import Foundation
import Testing
import UIKit
import UIKitExt
import PolkadotUI
import Products
@testable import polkadot_app

@MainActor
struct AppPermissionsPresenterTests {
    @Test
    func accountTransitionDropsOldUploadRowAndNeverRevokesRemoteAccess() throws {
        let interactor = Interactor()
        let view = View()
        let presenter = AppPermissionsPresenter(
            productName: "upload.product",
            interactor: interactor,
            wireframe: AppPermissionsWireframe(),
            viewModelFactory: AppPermissionsViewModelFactory()
        )
        presenter.view = view
        let remote = ProductPermissionGrant(
            productId: "upload.product", permission: .preimageSubmitAccess,
            granted: true, grantedAt: Date()
        )
        presenter.didReceive(grants: [remote])
        let first = scope(root: 1)
        presenter.didReceiveAutomaticUploads(scope: first, allowed: true)
        let oldRow = try #require(view.items.first { $0.id != remote.identifier })
        #expect(oldRow.isOn)

        // Lock/session replacement clears both the visible grant and its edit authority.
        presenter.didReceiveAutomaticUploads(scope: nil, allowed: false)
        #expect(view.items.map(\.id) == [remote.identifier])
        presenter.toggle(oldRow, isOn: false)
        #expect(interactor.writes.isEmpty)

        let second = scope(root: 2)
        presenter.didReceiveAutomaticUploads(scope: second, allowed: false)
        let newRow = try #require(view.items.first { $0.id != remote.identifier })
        #expect(!newRow.isOn)
        #expect(newRow.id != oldRow.id)
        presenter.toggle(oldRow, isOn: true)
        #expect(interactor.writes.isEmpty)
        presenter.toggle(newRow, isOn: true)
        #expect(interactor.writes.count == 1)
        #expect(interactor.writes.first?.scope == second)

        presenter.didReceiveAutomaticUploads(scope: second, allowed: true)
        let enabledRow = try #require(view.items.first { $0.id == newRow.id })
        presenter.toggle(enabledRow, isOn: false)
        #expect(interactor.writes.last?.allowed == false)
        #expect(interactor.writes.last?.scope == second)
        presenter.viewWillDisappear()
        #expect(interactor.revoked.isEmpty)
        #expect(view.items.first { $0.id == remote.identifier }?.isOn == true)
    }

    private func scope(root: UInt8) -> TrUAPIAutomaticUploadScope {
        TrUAPIAutomaticUploadScope(
            generation: UUID(), rootPublicKey: Data(repeating: root, count: 32),
            bulletinGenesis: Data(repeating: 3, count: 32)
        )
    }

    private final class View: UIViewController, AppPermissionsViewProtocol {
        var items: [AppPermissionsViewLayout.Item] = []
        func didReceive(items: [AppPermissionsViewLayout.Item]) { self.items = items }
        func setTitle(_: String) {}
    }

    private final class Interactor: AppPermissionsInteractorInputProtocol {
        var writes: [(allowed: Bool, scope: TrUAPIAutomaticUploadScope)] = []
        var revoked: [ProductPermission] = []
        func setup() {}
        func revokeOnDisappear(permissions: [ProductPermission]) { revoked = permissions }
        func setAutomaticUploads(allowed: Bool, scope: TrUAPIAutomaticUploadScope) {
            writes.append((allowed, scope))
        }
    }
}
