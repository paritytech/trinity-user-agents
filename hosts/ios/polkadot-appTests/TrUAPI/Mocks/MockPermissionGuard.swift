import Foundation
import Products
@testable import polkadot_app

final class MockPermissionGuard: ProductPermissionGuarding, @unchecked Sendable {
    var requestedPermission: ProductPermission?
    var requestedProductId: String?
    var requestedBatchedPermissions: [ProductPermission]?
    var verdictToReturn: Bool = true
    var decisionToReturn: PermissionDecision?

    func requestPermission(productId: String, permission: ProductPermission) async throws -> Bool {
        requestedProductId = productId
        requestedPermission = permission
        return verdictToReturn
    }

    func requestPermissionsBatched(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> Bool {
        requestedProductId = productId
        requestedBatchedPermissions = permissions
        return verdictToReturn
    }

    func requestDevicePermissionDecision(
        productId: String,
        capability: DeviceCapabilityType
    ) async throws -> PermissionDecision {
        requestedProductId = productId
        requestedPermission = .deviceCapability(capability)
        return decisionToReturn ?? (verdictToReturn ? .allowAlways : .deny)
    }

    func requestPermissionsDecision(
        productId: String,
        permissions: [ProductPermission]
    ) async throws -> PermissionDecision {
        requestedProductId = productId
        requestedBatchedPermissions = permissions
        return decisionToReturn ?? (verdictToReturn ? .allowAlways : .deny)
    }

    func consumePermission(productId _: String, permission _: ProductPermission) async throws -> Bool {
        verdictToReturn
    }

    func check(productId _: String, permission _: ProductPermission) async throws -> Bool {
        verdictToReturn
    }
}

final class MockOSPermissionAsker: OSPermissionAsking, @unchecked Sendable {
    var statusToReturn: OSPermissionStatus = .notDetermined
    var requestResult = false
    private(set) var checkedCapabilities: [DeviceCapabilityType] = []
    private(set) var requestedCapabilities: [DeviceCapabilityType] = []

    func checkPermission(for capability: DeviceCapabilityType) async -> OSPermissionStatus {
        checkedCapabilities.append(capability)
        return statusToReturn
    }

    func requestPermission(for capability: DeviceCapabilityType) async -> Bool {
        requestedCapabilities.append(capability)
        return requestResult
    }
}
