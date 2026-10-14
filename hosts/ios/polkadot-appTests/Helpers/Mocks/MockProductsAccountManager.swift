import Foundation
import Individuality
import Products
import SubstrateSdk
import UIKitExt

@testable import polkadot_app

// MARK: - Mock Products Account Manager

final class MockProductsAccountManager: ProductsAccountManaging, @unchecked Sendable {
    let subtreePublicKey = Data.random(of: 32)!
    var subtreeError: Error?
    private(set) var requestedProductIds: [ProductId] = []

    var isAllowanceSupported: Bool { false }

    func deriveAccount(_ productAccountId: ProductAccountId) throws -> AccountId {
        try Data(productAccountId.derivationPath().utf8)
    }

    func deriveProductSubtreePublicKey(for productId: ProductId) throws -> Data {
        requestedProductIds.append(productId)

        if let subtreeError {
            throw subtreeError
        }

        return subtreePublicKey
    }

    func requestResourceAllocation(
        for _: ProductId,
        resources: [AllocatableResource],
        policy _: OnExistingAllowancePolicy
    ) async throws -> [AllocationOutcome] {
        resources.map { _ in .notAvailable }
    }

    @MainActor func setPresentationView(_: ControllerBackedProtocol) {}
}
