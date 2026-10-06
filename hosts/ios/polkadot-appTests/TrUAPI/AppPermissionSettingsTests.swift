import Foundation
import PolkadotUI
import Products
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

@MainActor
struct AppPermissionSettingsTests {
    @Test func missingRuntimeCannotFallBackToLegacyPermissions() throws {
        #expect(throws: HostRejection.self) {
            try AppPermissionSettings(runtimeEnabled: true, runtimeProvider: nil)
        }
        guard case .legacy = try AppPermissionSettings(runtimeEnabled: false, runtimeProvider: nil) else {
            Issue.record("Disabled Rust settings must use the legacy store")
            return
        }
    }

    @Test func selectionRetainsCanonicalBundleAcrossSnapshotReordering() throws {
        let request = PermissionAuthorizationRequest.remote(.init(permission: .remote(domains: [
            "*.example.org", "api.example.net"
        ])))
        let bundle = AppPermissionRecord.rust(.init(productId: "product", request: request, status: .authorized))
        let camera = AppPermissionRecord.rust(.init(
            productId: "product",
            request: .device(.camera),
            status: .authorized
        ))
        let legacy = AppPermissionRecord.legacy(.init(
            productId: "product", permission: .balanceAccess, granted: true, grantedAt: Date(timeIntervalSince1970: 1)
        ))
        let refreshedLegacy = AppPermissionRecord.legacy(.init(
            productId: "product", permission: .balanceAccess, granted: true, grantedAt: Date(timeIntervalSince1970: 2)
        ))
        let interactor = SelectedSettingsRecords()
        let presenter = AppPermissionsPresenter(
            productName: "Product", interactor: interactor,
            wireframe: AppPermissionsWireframe(), viewModelFactory: AppPermissionsViewModelFactory()
        )
        let view = SettingsPermissionsView()
        presenter.view = view
        presenter.didReceive(grants: [bundle, camera, legacy])
        let item = try #require(view.items.first)
        #expect(item.description.contains("*.example.org, api.example.net"))
        presenter.toggle(item, isOn: false)
        try presenter.toggle(#require(view.items.last), isOn: false)
        presenter.didReceive(grants: [refreshedLegacy, camera, bundle])
        #expect(view.items.map(\.isOn) == [false, true, false])
        #expect(view.items.last?.id == item.id)
        presenter.viewWillDisappear()
        #expect(interactor.records == [refreshedLegacy, bundle])
    }

    @Test func disabledSettingsResetTheExistingCoreDataGrant() async throws {
        let storage = UserDataStorageTestFacade()
        let repository = ProductPermissionRepository(storageFacade: storage)
        try await repository.grant(productId: "product", permission: .deviceCapability(.notifications))
        let settings = AppPermissionSettings.legacy(
            ProductPermissionDataProviderFactory(storageFacade: storage), repository
        )
        var grants = try await settings.grantedRecords(productId: "product").makeAsyncIterator()
        let records = try #require(try await grants.next())
        #expect(records.map(\.productId) == ["product"])
        #expect(records.map(\.isNotifications) == [true])
        try await settings.revoke(records)
        #expect(try await grants.next() == [])
        #expect(try await repository.getPermissionState(
            productId: "product",
            permission: .deviceCapability(.notifications)
        ) == .notDetermined)
    }
}

private final class SelectedSettingsRecords: AppPermissionsInteractorInputProtocol {
    var records: [AppPermissionRecord] = []
    func setup() {}
    func revokeOnDisappear(records: [AppPermissionRecord]) { self.records = records }
}

@MainActor
private final class SettingsPermissionsView: UIViewController, AppPermissionsViewProtocol {
    var items: [AppPermissionsViewLayout.Item] = []
    func didReceive(items: [AppPermissionsViewLayout.Item]) { self.items = items }
    func setTitle(_: String) {}
}
