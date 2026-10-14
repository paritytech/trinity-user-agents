import Foundation
import Individuality
import KeyDerivation

public enum PreimageSubmitSponsorError: Error {
    case allocationRejected
    case allocationUnavailable
    case unexpectedAllocationOutcome
}

public final class PreimageSubmitSponsor: PreimageSubmitSponsoring {
    private let accountManager: ProductsAccountManaging
    private let resourceKeyManager: ProductResourceKeyManaging
    private let bulletInInfoProvider: BulletInSlotInfoProviding

    public init(
        accountManager: ProductsAccountManaging,
        resourceKeyManager: ProductResourceKeyManaging,
        bulletInInfoProvider: BulletInSlotInfoProviding
    ) {
        self.accountManager = accountManager
        self.resourceKeyManager = resourceKeyManager
        self.bulletInInfoProvider = bulletInInfoProvider
    }

    public func sponsor(productId: ProductId, data: Data) async throws -> any WalletManaging {
        let privateKey = try await ensureSlotKey(for: productId)

        let wallet = DynamicDerivedWallet(secretKeyProvider: { privateKey })
        let accountId = try wallet.getRawPublicKey()
        let allowance = try await bulletInInfoProvider.fetchAllowance(for: accountId)

        guard checkAllowanceExtensionNeed(from: allowance, data: data) else {
            return wallet
        }

        let extendedKey = try await extendAllowance(for: productId)
        return DynamicDerivedWallet(secretKeyProvider: { extendedKey })
    }
}

// MARK: - Private

private extension PreimageSubmitSponsor {
    func checkAllowanceExtensionNeed(from allowance: BulletInAllowanceInfo?, data: Data) -> Bool {
        guard let allowance else {
            return true
        }

        return allowance.isExpired || allowance.remainedSize < UInt64(data.count)
    }

    func ensureSlotKey(for productId: ProductId) async throws -> Data {
        if let cached = try resourceKeyManager.fetchResourceKey(for: productId, kind: .bulletIn) {
            return cached
        }

        return try await allocateAndPersist(productId: productId, policy: .ignore)
    }

    func extendAllowance(for productId: ProductId) async throws -> Data {
        try await allocateAndPersist(productId: productId, policy: .increase)
    }

    func allocateAndPersist(
        productId: ProductId,
        policy: OnExistingAllowancePolicy
    ) async throws -> Data {
        let outcomes = try await accountManager.requestResourceAllocation(
            for: productId,
            resources: [.bulletInAllowance],
            policy: policy
        )

        guard let outcome = outcomes.first else {
            throw PreimageSubmitSponsorError.unexpectedAllocationOutcome
        }

        switch outcome {
        case let .allocated(.bulletInAllowance(privateKey)):
            try resourceKeyManager.storeResourceKey(privateKey, for: productId, kind: .bulletIn)
            return privateKey
        case .rejected:
            throw PreimageSubmitSponsorError.allocationRejected
        case .notAvailable:
            throw PreimageSubmitSponsorError.allocationUnavailable
        default:
            throw PreimageSubmitSponsorError.unexpectedAllocationOutcome
        }
    }
}
