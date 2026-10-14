import Foundation
import Individuality
import KeyDerivation
import Products
import SubstrateSdk

/// RFC-0004 Accounts Protocol `get_account_alias` backed by host-held member keys.
protocol APAliasHandling: Sendable {
    func getContextualAlias(
        context: ProductProofContext,
        ring: RingLocation
    ) async throws -> ContextualAlias
}

final class APAliasHandler: APBaseMembershipHandler, @unchecked Sendable {
    private let accountAccessHandler: AccountAccessPermissionHandling

    init(
        callingProductId: ProductId?,
        options: [CreateProofOrAliasOption],
        depsResolver: MembershipDepsResolving,
        accountAccessHandler: AccountAccessPermissionHandling,
        logger: LoggerProtocol
    ) {
        self.accountAccessHandler = accountAccessHandler

        super.init(
            callingProductId: callingProductId,
            options: options,
            depsResolver: depsResolver,
            logger: logger
        )
    }
}

extension APAliasHandler: APAliasHandling {
    func getContextualAlias(
        context: ProductProofContext,
        ring: RingLocation
    ) async throws -> ContextualAlias {
        do {
            return try await performGetContextualAlias(context: context, ring: ring)
        } catch {
            logger.error("Get contextual alias failed: \(error)")
            throw GetAliasError.wrapping(error)
        }
    }
}

private extension APAliasHandler {
    // Derive-only by design: an alias is available even before the user reaches
    // full personhood, so ring membership is not checked here.
    func performGetContextualAlias(
        context: ProductProofContext,
        ring: RingLocation
    ) async throws -> ContextualAlias {
        try await ensureAliasAccess(for: context)

        guard try await isValidRing(ring) else {
            throw GetAliasError.ringNotFound
        }

        guard let collectionId = requestedCollection(for: ring),
              let option = options.first(where: { $0.collectionId == collectionId }) ?? options.first else {
            throw GetAliasError.unknown("No ring keys configured")
        }

        let contextBytes = try context.contextBytes()
        let alias = try option.keyManager.deriveAlias(for: contextBytes)

        return ContextualAlias(context: contextBytes, alias: alias)
    }

    // Deriving an alias under another product's context reuses the persisted
    // account-access grant; same-product requests short-circuit in the handler.
    func ensureAliasAccess(for context: ProductProofContext) async throws {
        guard let callingProductId else {
            throw GetAliasError.rejected
        }

        let granted = try await accountAccessHandler.request(
            productId: callingProductId,
            targetProductId: context.productId
        )

        guard granted else {
            throw GetAliasError.rejected
        }
    }
}
