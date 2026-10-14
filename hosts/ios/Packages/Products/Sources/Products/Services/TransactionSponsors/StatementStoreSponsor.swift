import Foundation
import Individuality
import KeyDerivation

public enum StatementStoreSponsorError: Error {
    case allocationRejected
    case allocationUnavailable
    case unexpectedAllocationOutcome
}

public final class StatementStoreSponsor: StatementStoreSponsoring {
    private let accountManager: ProductsAccountManaging
    private let resourceKeyManager: ProductResourceKeyManaging
    private let slotInfoProvider: StatementStoreSlotInfoProviding

    public init(
        accountManager: ProductsAccountManaging,
        resourceKeyManager: ProductResourceKeyManaging,
        slotInfoProvider: StatementStoreSlotInfoProviding
    ) {
        self.accountManager = accountManager
        self.resourceKeyManager = resourceKeyManager
        self.slotInfoProvider = slotInfoProvider
    }

    public func sponsor(productId: ProductId) async throws -> any WalletManaging {
        let privateKey = try await ensureSlotKey(for: productId)

        let wallet = DynamicDerivedWallet(secretKeyProvider: { privateKey })
        let accountId = try wallet.getRawPublicKey()

        let hasSlot = try await slotInfoProvider.hasExistingSlot(for: accountId)

        guard hasSlot else {
            let freshKey = try await allocateAndPersist(for: productId)
            return DynamicDerivedWallet(secretKeyProvider: { freshKey })
        }

        return wallet
    }
}

// MARK: - Private

private extension StatementStoreSponsor {
    func ensureSlotKey(for productId: ProductId) async throws -> Data {
        if let cached = try resourceKeyManager.fetchResourceKey(
            for: productId,
            kind: .statementStore
        ) {
            return cached
        }

        return try await allocateAndPersist(for: productId)
    }

    func allocateAndPersist(for productId: ProductId) async throws -> Data {
        let outcomes = try await accountManager.requestResourceAllocation(
            for: productId,
            resources: [.statementStoreAllowance],
            policy: .ignore
        )

        guard let outcome = outcomes.first else {
            throw StatementStoreSponsorError.unexpectedAllocationOutcome
        }

        switch outcome {
        case let .allocated(.statementStoreAllowance(privateKey)):
            try resourceKeyManager.storeResourceKey(
                privateKey,
                for: productId,
                kind: .statementStore
            )
            return privateKey
        case .rejected:
            throw StatementStoreSponsorError.allocationRejected
        case .notAvailable:
            throw StatementStoreSponsorError.allocationUnavailable
        default:
            throw StatementStoreSponsorError.unexpectedAllocationOutcome
        }
    }
}
