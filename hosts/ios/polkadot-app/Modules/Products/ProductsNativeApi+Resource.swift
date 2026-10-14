import Foundation
import Products

// MARK: - Entropy Derivation

extension ProductsNativeApi {
    func deriveEntropy(key: Data) async throws -> Data {
        try entropyDeriver.deriveEntropy(productId: productId, key: key)
    }
}

// MARK: - Resource Allocation

extension ProductsNativeApi {
    func requestResourceAllocation(
        resources: [AllocatableResource]
    ) async throws -> [AllocationOutcome] {
        guard accountManager.isAllowanceSupported else {
            logger.error("Resource allocation requested but allowance support is unavailable")
            throw ProductNativeApiError.accountServicesNotSupported
        }

        let outcomes = try await accountManager.requestResourceAllocation(
            for: productId,
            resources: resources,
            policy: .increase
        )

        for outcome in outcomes {
            try persistAllocatedKeys(outcome: outcome)
        }

        return outcomes
    }
}

// MARK: - Key Persistence

private extension ProductsNativeApi {
    func persistAllocatedKeys(outcome: AllocationOutcome) throws {
        guard case let .allocated(resource) = outcome else { return }

        switch resource {
        case let .statementStoreAllowance(privateKey):
            try resourceKeyManager.storeResourceKey(
                privateKey,
                for: productId,
                kind: .statementStore
            )
        case let .bulletInAllowance(privateKey):
            try resourceKeyManager.storeResourceKey(
                privateKey,
                for: productId,
                kind: .bulletIn
            )
        case .autoSigning,
             .smartContractAllowance:
            break
        }
    }
}
