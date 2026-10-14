import Foundation
import SubstrateSdk
import KeyDerivation
import Individuality
import UIKitExt

public final class ProductsAccountManager: @unchecked Sendable {
    private let accountHolder: ProductAccountHolding
    private let allowanceSupport: AllowanceSupport?

    public init(
        entropyManager: RootEntropyManaging,
        allowanceSupport: AllowanceSupport?
    ) {
        accountHolder = ProductAccountHolder(entropyManager: entropyManager)
        self.allowanceSupport = allowanceSupport
    }
}

extension ProductsAccountManager: ProductsAccountManaging {
    public var isAllowanceSupported: Bool {
        allowanceSupport != nil
    }

    public func deriveAccount(_ productAccountId: ProductAccountId) throws -> AccountId {
        try accountHolder.deriveAccount(productAccountId)
    }

    public func deriveProductSubtreePublicKey(for productId: ProductId) throws -> Data {
        try accountHolder.deriveProductSubtreePublicKey(for: productId)
    }

    @MainActor
    public func setPresentationView(_ view: ControllerBackedProtocol) {
        allowanceSupport?.allowancePromptRouter.setPresentationView(view)
    }

    public func requestResourceAllocation(
        for productId: ProductId,
        resources: [AllocatableResource],
        policy: OnExistingAllowancePolicy
    ) async throws -> [AllocationOutcome] {
        guard let allowanceSupport else {
            return resources.map { _ in .notAvailable }
        }

        guard !resources.isEmpty else { return [] }

        let approved = await awaitUserApproval(
            allowanceSupport: allowanceSupport,
            productId: productId,
            resources: resources
        )

        guard approved else {
            return resources.map { _ in .rejected }
        }

        return try await allocateAll(
            allowanceSupport: allowanceSupport,
            productId: productId,
            resources: resources,
            policy: policy
        )
    }
}

// MARK: - Private

private extension ProductsAccountManager {
    func awaitUserApproval(
        allowanceSupport: AllowanceSupport,
        productId: ProductId,
        resources: [AllocatableResource]
    ) async -> Bool {
        let decision = await withCheckedContinuation { continuation in
            Task { @MainActor [allowancePromptRouter = allowanceSupport.allowancePromptRouter] in
                let context = AllowancePromptContext(
                    productId: productId,
                    resources: resources
                )
                context.setContinuation(continuation)
                allowancePromptRouter.showAllowancePrompt(context: context)
            }
        }

        return decision == .approved
    }

    func allocateAll(
        allowanceSupport: AllowanceSupport,
        productId: ProductId,
        resources: [AllocatableResource],
        policy: OnExistingAllowancePolicy
    ) async throws -> [AllocationOutcome] {
        var outcomes: [AllocationOutcome] = []

        for resource in resources {
            let outcome = await allocate(
                allowanceSupport: allowanceSupport,
                productId: productId,
                resource: resource,
                policy: policy
            )
            outcomes.append(outcome)
        }

        return outcomes
    }

    func allocate(
        allowanceSupport: AllowanceSupport,
        productId: ProductId,
        resource: AllocatableResource,
        policy: OnExistingAllowancePolicy
    ) async -> AllocationOutcome {
        do {
            switch resource {
            case .autoSigning:
                let secrets = try accountHolder.deriveAutoSigningSecrets(for: productId)
                return .allocated(.autoSigning(secrets))
            case .statementStoreAllowance:
                let wallet = try accountHolder.deriveStatementStoreAccount(for: productId)
                let accountId = try wallet.getRawPublicKey()
                try await allowanceSupport.sssManager.allocate(
                    accountId: accountId,
                    policy: policy,
                    priority: .normal
                )
                let privateKey = try wallet.fetchRawSecretKey()
                return .allocated(.statementStoreAllowance(privateKey: privateKey))
            case .bulletInAllowance:
                let wallet = try accountHolder.deriveBulletInAccount(for: productId)
                let accountId = try wallet.getRawPublicKey()
                try await allowanceSupport.bulletInManager.allocate(
                    accountId: accountId,
                    policy: policy,
                    priority: .normal
                )
                let privateKey = try wallet.fetchRawSecretKey()
                return .allocated(.bulletInAllowance(privateKey: privateKey))
            case let .smartContractAllowance(dest):
                let wallet = try accountHolder.deriveSmartContractAccount(
                    for: productId,
                    derivationIndex: dest
                )
                let accountId = try wallet.getRawPublicKey()
                try await allowanceSupport.smartContractManager.allocate(
                    accountId: accountId,
                    policy: policy,
                    priority: .normal
                )
                return .allocated(.smartContractAllowance)
            }
        } catch {
            return .notAvailable
        }
    }
}
