import Foundation
import Products
import Testing
import TrUAPIHost
import Operation_iOS
import PolkadotUI
import UIKit
import UIKitExt

@testable import polkadot_app

@Suite("ProductPermissionRepository Tests")
struct ProductPermissionRepositoryTests {
    private func makeSUT() -> ProductPermissionRepository {
        let authority = PermissionAuthorityFixture()
        return ProductPermissionRepository(storageFacade: UserDataStorageTestFacade(), authority: { authority })
    }

    // MARK: - grant / getPermissionState

    @Test("grant persists and getPermissionState returns allowedAlways")
    func grantAndGetState() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.camera)

        let initial = try await sut.getPermissionState(
            productId: productId,
            permission: permission
        )
        #expect(initial == .notDetermined)

        try await sut.grant(productId: productId, permission: permission)

        let granted = try await sut.getPermissionState(
            productId: productId,
            permission: permission
        )
        #expect(granted == .allowedAlways)
    }

    @Test("getPermissionState returns notDetermined for different product")
    func getStateIsolatedByProduct() async throws {
        let sut = makeSUT()
        let permission = ProductPermission.deviceCapability(.camera)

        try await sut.grant(productId: "product-a", permission: permission)

        let state = try await sut.getPermissionState(
            productId: "product-b",
            permission: permission
        )
        #expect(state == .notDetermined)
    }

    @Test("getPermissionState returns notDetermined for different permission")
    func getStateIsolatedByPermission() async throws {
        let sut = makeSUT()
        let productId = "test-product"

        try await sut.grant(productId: productId, permission: .deviceCapability(.camera))

        let state = try await sut.getPermissionState(
            productId: productId,
            permission: .deviceCapability(.microphone)
        )
        #expect(state == .notDetermined)
    }

    @Test("JAM grants and revocation distinguish full genesis and product")
    func jamGrantsAreScopedToFullGenesisAndProduct() async throws {
        let sut = makeSUT()
        let first = ProductPermission.jamPeersAccess(
            genesis: "0x10c123f0" + String(repeating: "ab", count: 28)
        )
        let second = ProductPermission.jamPeersAccess(
            genesis: "0x10c123f0" + String(repeating: "cd", count: 28)
        )

        try await sut.grant(productId: "product-a", permission: first)
        #expect(try await sut.getPermissionState(
            productId: "product-a", permission: second
        ) == .notDetermined)
        #expect(try await sut.getPermissionState(
            productId: "product-b", permission: first
        ) == .notDetermined)

        try await sut.grant(productId: "product-a", permission: second)
        let restored = try await sut.getAllByProduct(productId: "product-a")
        #expect(restored.contains { $0.permission == first })
        #expect(restored.contains { $0.permission == second })

        try await sut.revoke(productId: "product-a", permission: first)
        #expect(try await sut.getPermissionState(
            productId: "product-a", permission: first
        ) == .notDetermined)
        #expect(try await sut.getPermissionState(
            productId: "product-a", permission: second
        ) == .allowedAlways)
    }

    // MARK: - deny

    @Test("deny persists and getPermissionState returns denied")
    func denyAndGetState() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.networkAccess(domain: "evil.com")

        try await sut.deny(productId: productId, permission: permission)

        let state = try await sut.getPermissionState(
            productId: productId,
            permission: permission
        )
        #expect(state == .denied)
    }

    @Test("deny overrides previously granted permission")
    func denyOverridesGrant() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.camera)

        try await sut.grant(productId: productId, permission: permission)
        #expect(
            try await sut.getPermissionState(
                productId: productId,
                permission: permission
            ) == .allowedAlways
        )

        try await sut.deny(productId: productId, permission: permission)
        #expect(
            try await sut.getPermissionState(
                productId: productId,
                permission: permission
            ) == .denied
        )
    }

    @Test("deny clears one-time grant")
    func denyClearsOneTime() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.camera)

        sut.grantOneTime(productId: productId, permission: permission)

        try await sut.deny(productId: productId, permission: permission)

        #expect(!sut.consumeOneTimeGrant(productId: productId, permission: permission))
    }

    // MARK: - getPermissionState with one-time grant

    @Test("getPermissionState returns allowedOnce for one-time grant")
    func getStateOneTimeGrant() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.camera)

        sut.grantOneTime(productId: productId, permission: permission)

        let state = try await sut.getPermissionState(
            productId: productId,
            permission: permission
        )
        #expect(state == .allowedOnce)
    }

    // MARK: - revoke

    @Test("revoke removes a granted permission")
    func revokeSingle() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.networkAccess(domain: "example.com")

        try await sut.grant(productId: productId, permission: permission)
        #expect(
            try await sut.getPermissionState(
                productId: productId,
                permission: permission
            ).isAllowed
        )

        try await sut.revoke(productId: productId, permission: permission)
        #expect(
            try await sut.getPermissionState(
                productId: productId,
                permission: permission
            ) == .notDetermined
        )
    }

    @Test("revoke clears one-time grant")
    func revokeClearsOneTime() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.networkAccess(domain: "example.com")

        sut.grantOneTime(productId: productId, permission: permission)

        try await sut.revoke(productId: productId, permission: permission)

        #expect(!sut.consumeOneTimeGrant(productId: productId, permission: permission))
    }

    // MARK: - revokeAllByProduct

    @Test("revokeAllByProduct removes all grants for given product only")
    func revokeAllByProduct() async throws {
        let sut = makeSUT()
        let camera = ProductPermission.deviceCapability(.camera)
        let network = ProductPermission.networkAccess(domain: "api.example.com")

        try await sut.grant(productId: "product-a", permission: camera)
        try await sut.grant(productId: "product-a", permission: network)
        try await sut.grant(productId: "product-b", permission: camera)

        try await sut.revokeAllByProduct(productId: "product-a")

        let stateA1 = try await sut.getPermissionState(
            productId: "product-a",
            permission: camera
        )
        let stateA2 = try await sut.getPermissionState(
            productId: "product-a",
            permission: network
        )
        let stateB = try await sut.getPermissionState(
            productId: "product-b",
            permission: camera
        )

        #expect(stateA1 == .notDetermined)
        #expect(stateA2 == .notDetermined)
        #expect(stateB == .allowedAlways)
    }

    // MARK: - getAllByProduct

    @Test("getAllByProduct returns only grants for given product")
    func getAllByProduct() async throws {
        let sut = makeSUT()
        let camera = ProductPermission.deviceCapability(.camera)
        let mic = ProductPermission.deviceCapability(.microphone)

        try await sut.grant(productId: "product-a", permission: camera)
        try await sut.grant(productId: "product-a", permission: mic)
        try await sut.grant(productId: "product-b", permission: camera)

        let grants = try await sut.getAllByProduct(productId: "product-a")
        #expect(grants.count == 2)
        #expect(grants.allSatisfy { $0.productId == "product-a" })
    }

    @Test("getAllByProduct returns empty for unknown product")
    func getAllByProductEmpty() async throws {
        let sut = makeSUT()

        let grants = try await sut.getAllByProduct(productId: "nonexistent")
        #expect(grants.isEmpty)
    }

    // MARK: - isAnyAlwaysGranted

    @Test("isAnyAlwaysGranted returns true when one of the keys matches")
    func isAnyAlwaysGrantedMatch() async throws {
        let sut = makeSUT()
        let productId = "test-product"

        try await sut.grant(
            productId: productId,
            permission: .networkAccess(domain: "example.com")
        )

        let result = try await sut.isAnyAlwaysGranted(
            productId: productId,
            typeName: ProductPermission.networkAccessTypeName,
            keys: ["other.com", "example.com"]
        )

        #expect(result)
    }

    @Test("isAnyAlwaysGranted returns false when no keys match")
    func isAnyAlwaysGrantedNoMatch() async throws {
        let sut = makeSUT()
        let productId = "test-product"

        try await sut.grant(
            productId: productId,
            permission: .networkAccess(domain: "example.com")
        )

        let result = try await sut.isAnyAlwaysGranted(
            productId: productId,
            typeName: ProductPermission.networkAccessTypeName,
            keys: ["other.com", "unknown.com"]
        )

        #expect(!result)
    }

    @Test("isAnyAlwaysGranted returns false for empty keys")
    func isAnyAlwaysGrantedEmptyKeys() async throws {
        let sut = makeSUT()

        let result = try await sut.isAnyAlwaysGranted(
            productId: "test-product",
            typeName: ProductPermission.networkAccessTypeName,
            keys: []
        )

        #expect(!result)
    }

    @Test("isAnyAlwaysGranted ignores denied permissions")
    func isAnyAlwaysGrantedIgnoresDenied() async throws {
        let sut = makeSUT()
        let productId = "test-product"

        try await sut.deny(
            productId: productId,
            permission: .networkAccess(domain: "denied.com")
        )

        let result = try await sut.isAnyAlwaysGranted(
            productId: productId,
            typeName: ProductPermission.networkAccessTypeName,
            keys: ["denied.com"]
        )

        #expect(!result)
    }

    // MARK: - One-time grants

    @Test("grantOneTime and consumeOneTimeGrant work as expected")
    func oneTimeGrantConsumed() {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.camera)

        #expect(!sut.consumeOneTimeGrant(productId: productId, permission: permission))

        sut.grantOneTime(productId: productId, permission: permission)
        #expect(sut.consumeOneTimeGrant(productId: productId, permission: permission))
        #expect(!sut.consumeOneTimeGrant(productId: productId, permission: permission))
    }

    @Test("one-time grant does not affect persistent storage")
    func oneTimeGrantNotPersisted() async throws {
        let sut = makeSUT()
        let productId = "test-product"
        let permission = ProductPermission.deviceCapability(.microphone)

        sut.grantOneTime(productId: productId, permission: permission)

        let grants = try await sut.getAllByProduct(productId: productId)
        #expect(grants.isEmpty)
    }

    @Test("one-time grants are isolated by product")
    func oneTimeGrantIsolatedByProduct() {
        let sut = makeSUT()
        let permission = ProductPermission.deviceCapability(.camera)

        sut.grantOneTime(productId: "product-a", permission: permission)

        #expect(!sut.consumeOneTimeGrant(productId: "product-b", permission: permission))
        #expect(sut.consumeOneTimeGrant(productId: "product-a", permission: permission))
    }
}

/// Host-only fixture: models atomic import/CAS and explicit reset tombstones.
private final class PermissionAuthorityFixture: ProductPermissionAuthority, @unchecked Sendable {
    private let lock = NSLock()
    private var records: [String: [PermissionAuthorizationEntry]] = [:]
    private var revisions: [String: UInt64] = [:]
    var failWrites = false

    func permissionAuthorizationProducts() async throws -> [String] {
        lock.withLock { Array(records.keys) }
    }

    func permissionAuthorizations(productId: String) async throws -> [PermissionAuthorizationEntry] {
        lock.withLock { records[productId] ?? [] }
    }

    func permissionAuthorizationRevision(productId: String) throws -> UInt64 {
        lock.withLock { revisions[productId, default: 0] }
    }

    func importPermissionAuthorizations(
        productId: String, entries: [PermissionAuthorizationEntry]
    ) async throws -> [PermissionAuthorizationEntry] {
        lock.withLock {
            for entry in entries where !(records[productId] ?? []).contains(where: { $0.request == entry.request }) {
                records[productId, default: []].append(entry)
            }
            return records[productId] ?? []
        }
    }

    func setPermissionAuthorizationStatus(
        productId: String, request: PermissionAuthorizationRequest, status: PermissionAuthorizationStatus
    ) async throws {
        try lock.withLock {
            revisions[productId, default: 0] += 1
            try persist(productId: productId, request: request, status: status)
        }
    }

    func setPermissionAuthorizationStatusIfCurrent(
        productId: String, request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus, revision: UInt64
    ) async throws -> Bool {
        try lock.withLock {
            guard revisions[productId, default: 0] == revision else { return false }
            if status != .authorized { revisions[productId, default: 0] += 1 }
            try persist(productId: productId, request: request, status: status)
            return true
        }
    }

    private func persist(
        productId: String, request: PermissionAuthorizationRequest, status: PermissionAuthorizationStatus
    ) throws {
        if failWrites { throw NSError(domain: "permission write failed", code: 1) }
        records[productId, default: []].removeAll { $0.request == request }
        records[productId, default: []].append(.init(request: request, status: status))
    }
}

private struct PermissionPromptFixture: ProductPermissionRequesting {
    let decision: Products.PermissionDecision
    let whilePrompting: @Sendable () async throws -> Void

    func prompt(productId: String, permission: ProductPermission) async -> Products.PermissionDecision {
        await promptBatched(productId: productId, permissions: [permission])
    }

    func promptBatched(productId _: String, permissions _: [ProductPermission]) async -> Products.PermissionDecision {
        do { try await whilePrompting() } catch { Issue.record(error) }
        return decision
    }
}

extension ProductPermissionRepositoryTests {
    @Test("Existing native records win; missing legacy records import; reset cannot reimport")
    func nativeAuthorityAndLegacyImport() async throws {
        let storage = UserDataStorageTestFacade()
        let authority = PermissionAuthorityFixture()
        let camera = ProductPermission.deviceCapability(.camera)
        let microphone = ProductPermission.deviceCapability(.microphone)
        let legacy = AnyDataProviderRepository(
            storage.createRepository(mapper: AnyCoreDataMapper(ProductPermissionGrantMapper()))
        )
        let rows = [camera, microphone, .balanceAccess].map {
            ProductPermissionGrant(productId: "demo.paseo", permission: $0, granted: true, grantedAt: nil)
        }
        try await legacy.saveOperation({ rows }, { [] }).asyncExecute()
        try await authority.setPermissionAuthorizationStatus(
            productId: "demo.paseo", request: .device(.camera), status: .denied
        )
        try await authority.setPermissionAuthorizationStatus(
            productId: "demo.paseo", request: .identityDisclosure, status: .authorized
        )
        let sut = ProductPermissionRepository(storageFacade: storage, authority: { authority })
        let grants = try await sut.getAllByProduct(productId: "demo.paseo")
        #expect(grants.count == 4)
        #expect(grants.first { $0.permission == camera }?.granted == false)
        #expect(grants.first { $0.permission == microphone }?.granted == true)
        #expect(grants.first { $0.permission == .balanceAccess }?.granted == true)
        #expect(grants.first { $0.permission == .userIdentityAccess }?.granted == true)
        try await sut.revoke(productId: "demo.paseo", permission: microphone)
        #expect(try await sut.getPermissionState(productId: "demo.paseo", permission: microphone) == .notDetermined)
        #expect(try await sut.getPermissionState(productId: "demo.paseo", permission: microphone) == .notDetermined)
        #expect(try await sut.settingsGrants(legacy: []).contains { $0.permission == .userIdentityAccess })
    }

    @Test("A domain bundle retains its exact native identity")
    func nativeBundleIsNotFlattened() async throws {
        let authority = PermissionAuthorityFixture()
        let request = PermissionAuthorizationRequest.remote(.init(permission: .remote(domains: ["a.test", "b.test"])))
        try await authority.setPermissionAuthorizationStatus(productId: "demo.paseo", request: request, status: .denied)
        let sut = ProductPermissionRepository(storageFacade: UserDataStorageTestFacade(), authority: { authority })
        let grants = try await sut.getAllByProduct(productId: "demo.paseo")
        let grant = try #require(grants.first)
        #expect(grants.count == 1)
        #expect(grant.permission == .networkAccessBundle(domains: ["a.test", "b.test"]))
        #expect(try await sut.getPermissionState(productId: "demo.paseo", permission: .networkAccess(domain: "a.test")) == .notDetermined)
        try await sut.revoke(productId: "demo.paseo", permission: grant.permission)
        let entries = try await authority.permissionAuthorizations(productId: "demo.paseo")
        #expect(entries.count == 1)
        #expect(entries.first?.request == request)
        #expect(entries.first?.status == .notDetermined)
    }

    @Test("Revocation wins against pending legacy prompts", arguments: [Products.PermissionDecision.allowAlways, .allowOnce])
    func pendingLegacyPromptCannotResurrect(decision: Products.PermissionDecision) async throws {
        let authority = PermissionAuthorityFixture()
        let storage = UserDataStorageTestFacade()
        let reader = ProductPermissionRepository(storageFacade: storage, authority: { authority })
        let settings = ProductPermissionRepository(storageFacade: storage, authority: { authority })
        let permission = ProductPermission.networkAccess(domain: "example.com")
        let requester = PermissionPromptFixture(decision: decision) {
            try await settings.revoke(productId: "demo.paseo", permission: permission)
        }
        #expect(try await reader.promptPermission(productId: "demo.paseo", permission: permission, requester: requester) == false)
        #expect(try await reader.getPermissionState(productId: "demo.paseo", permission: permission) == .notDetermined)
        #expect(!reader.consumeOneTimeGrant(productId: "demo.paseo", permission: permission))
    }

    @Test("Revocation invalidates sibling one-time grants without touching another product")
    func siblingTemporaryGrantsFollowCoreRevision() async throws {
        let authority = PermissionAuthorityFixture()
        let reader = ProductPermissionRepository(storageFacade: UserDataStorageTestFacade(), authority: { authority })
        let settings = ProductPermissionRepository(storageFacade: UserDataStorageTestFacade(), authority: { authority })
        let permission = ProductPermission.deviceCapability(.camera)
        reader.grantOneTime(productId: "demo.paseo", permission: permission)
        reader.grantOneTime(productId: "other.paseo", permission: permission)
        try await settings.revoke(productId: "demo.paseo", permission: permission)
        #expect(!reader.consumeOneTimeGrant(productId: "demo.paseo", permission: permission))
        #expect(reader.consumeOneTimeGrant(productId: "other.paseo", permission: permission))
    }

    @Test("Domain and account mapping retain existing core scopes")
    func canonicalPermissionMapping() throws {
        #expect(try ProductPermission.networkAccess(domain: " *.Example.COM. ").canonicalPermission() == .networkAccess(domain: "*.example.com"))
        #expect(try ProductPermission.networkAccess(domain: "Bücher.example").canonicalPermission() == .networkAccess(domain: "xn--bcher-kva.example"))
        #expect(try ProductPermission.accountAccess(targetProductId: "app.peopl.paseo").authorizationRequest() == .accountAccess(targetProductId: "peopl"))
        #expect(throws: ProductPermissionMappingError.self) {
            try ProductPermission.networkAccess(domain: "https://example.com/path").authorizationRequest()
        }
    }

    @Test("JAM permission mapping preserves the full genesis and rejects short keys")
    func canonicalJamPermissionMapping() throws {
        let permission = ProductPermission.jamPeersAccess(genesis: "0x" + String(repeating: "ab", count: 32))
        let request = try #require(try permission.authorizationRequest())
        #expect(try ProductPermission.fromAuthorization(request) == [permission])
        #expect(throws: ProductPermissionMappingError.self) {
            try ProductPermission.jamPeersAccess(genesis: "0xab").authorizationRequest()
        }
    }
}

@MainActor
private final class PermissionSettingsViewFixture: @MainActor AppPermissionsViewProtocol {
    let controller = UIViewController()
    var isSetup: Bool { true }
    var items: [AppPermissionsViewLayout.Item] = []
    var revoking = false
    func didReceive(items: [AppPermissionsViewLayout.Item]) { self.items = items }
    func setTitle(_: String) {}
    func setRevoking(_ revoking: Bool) { self.revoking = revoking }
}

private final class PermissionSettingsInteractorFixture: AppPermissionsInteractorInputProtocol {
    var requests: [[ProductPermission]] = []
    func setup() {}
    func revoke(permissions: [ProductPermission]) { requests.append(permissions) }
}

@MainActor
private final class PermissionSettingsWireframeFixture: AppPermissionsWireframeProtocol {
    var errors: [String] = []
    func present(message: String?, title _: String?, closeAction _: String?, from _: ControllerBackedProtocol?) {
        errors.append(message ?? "")
    }
}

extension ProductPermissionRepositoryTests {
    @Test("Settings retain the granted switch until acknowledgement and display failures")
    @MainActor
    func settingsRevokeIsNotOptimistic() throws {
        let view = PermissionSettingsViewFixture()
        let interactor = PermissionSettingsInteractorFixture()
        let wireframe = PermissionSettingsWireframeFixture()
        let presenter = AppPermissionsPresenter(
            productName: "Demo", interactor: interactor, wireframe: wireframe,
            viewModelFactory: AppPermissionsViewModelFactory()
        )
        presenter.view = view
        presenter.didReceive(grants: [
            ProductPermissionGrant(productId: "demo.paseo", permission: .deviceCapability(.camera), granted: true, grantedAt: nil)
        ])
        let item = try #require(view.items.first)
        presenter.toggle(item, isOn: false)
        #expect(view.revoking)
        #expect(view.items.first?.isOn == true)
        #expect(interactor.requests == [[.deviceCapability(.camera)]])
        presenter.didFinishRevoking()
        presenter.didReceive(error: NSError(domain: "storage unavailable", code: 1))
        #expect(!view.revoking)
        #expect(view.items.first?.isOn == true)
        #expect(wireframe.errors.count == 1)
        presenter.toggle(item, isOn: false)
        presenter.didReceive(grants: [])
        presenter.didFinishRevoking()
        #expect(view.items.isEmpty)
        #expect(!view.revoking)
    }

    @Test("Failed runtime mutation is reported and does not remove the stored grant")
    func failedRevokeDoesNotClaimSuccess() async throws {
        let authority = PermissionAuthorityFixture()
        let sut = ProductPermissionRepository(storageFacade: UserDataStorageTestFacade(), authority: { authority })
        try await sut.grant(productId: "demo.paseo", permission: .deviceCapability(.camera))
        authority.failWrites = true
        await #expect(throws: NSError.self) {
            try await sut.revoke(productId: "demo.paseo", permission: .deviceCapability(.camera))
        }
        #expect(try await sut.getPermissionState(productId: "demo.paseo", permission: .deviceCapability(.camera)) == .allowedAlways)
    }
}

extension ProductPermissionRepositoryTests {
    @Test(.timeLimit(.minutes(1))) func nativeOnlyGrantSubscriptionsObserveRevocation() async throws {
        let authority = PermissionAuthorityFixture()
        let storage = UserDataStorageTestFacade()
        let repository = ProductPermissionRepository(storageFacade: storage, authority: { authority })
        let factory = ProductPermissionDataProviderFactory(storageFacade: storage, permissionRepository: repository)
        try await authority.setPermissionAuthorizationStatus(
            productId: "native.paseo", request: .device(.camera), status: .authorized
        )
        var iterator = factory.subscribeAllGrants(grantedOnly: true).makeAsyncIterator()
        let first = try await iterator.next()
        let initial = try #require(first)
        #expect(initial.contains { $0.productId == "native.paseo" && $0.permission == .deviceCapability(.camera) })
        try await repository.revoke(productId: "native.paseo", permission: .deviceCapability(.camera))
        NotificationCenter.default.post(name: .productPermissionAuthorizationsChanged, object: "native.paseo")
        while let snapshot = try await iterator.next() {
            if snapshot.isEmpty { return }
        }
        Issue.record("Canonical permission subscription ended without the revoked snapshot")
    }
}

@MainActor
private final class PermissionSettingsOutputFixture: AppPermissionsInteractorOutputProtocol {
    var snapshots: [[ProductPermissionGrant]] = []
    var finished = false
    let errorContinuation: AsyncStream<String>.Continuation

    init(errorContinuation: AsyncStream<String>.Continuation) {
        self.errorContinuation = errorContinuation
    }

    func didReceive(grants: [ProductPermissionGrant]) { snapshots.append(grants) }
    func didFinishRevoking() { finished = true }
    func didReceive(error: Error) { errorContinuation.yield(error.localizedDescription) }
}

extension ProductPermissionRepositoryTests {
    @Test(.timeLimit(.minutes(1))) @MainActor
    func interactorDeliversRevokeFailureWhileSettingsArePresent() async throws {
        let authority = PermissionAuthorityFixture()
        let storage = UserDataStorageTestFacade()
        let repository = ProductPermissionRepository(storageFacade: storage, authority: { authority })
        try await repository.grant(productId: "demo.paseo", permission: .deviceCapability(.camera))
        authority.failWrites = true
        let interactor = AppPermissionsInteractor(
            productId: "demo.paseo",
            providerFactory: ProductPermissionDataProviderFactory(storageFacade: storage, permissionRepository: repository),
            repository: repository,
            notificationScheduler: MockNotificationScheduler()
        )
        let (errors, continuation) = AsyncStream.makeStream(of: String.self)
        let output = PermissionSettingsOutputFixture(errorContinuation: continuation)
        interactor.presenter = output
        interactor.revoke(permissions: [.deviceCapability(.camera)])
        var iterator = errors.makeAsyncIterator()
        let error = await iterator.next()
        #expect(error != nil)
        #expect(output.finished)
        #expect(output.snapshots.isEmpty)
        continuation.finish()
    }
}
