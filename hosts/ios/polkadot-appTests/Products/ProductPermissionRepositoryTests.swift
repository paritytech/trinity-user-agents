import Foundation
import Products
import Testing

@testable import polkadot_app

@Suite("ProductPermissionRepository Tests")
struct ProductPermissionRepositoryTests {
    private func makeSUT() -> ProductPermissionRepository {
        ProductPermissionRepository(storageFacade: UserDataStorageTestFacade())
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
