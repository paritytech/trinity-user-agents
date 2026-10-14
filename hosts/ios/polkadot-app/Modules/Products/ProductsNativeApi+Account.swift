import Foundation
import Products

// MARK: - Account & Chain

extension ProductsNativeApi {
    func accountGet(_ accountId: ProductAccountId) async throws -> ProductAccountResult {
        guard try await permissionGuard.consumePermission(
            productId: productId,
            permission: .accountAccess(targetProductId: accountId.productId)
        ) else {
            throw ProductNativeApiError.permissionDenied
        }

        let publicKey = try accountManager.deriveAccount(accountId)
        return ProductAccountResult(
            publicKey: publicKey.toHex(includePrefix: true)
        )
    }

    func getUserId() async throws -> GetUserIdResult {
        guard let username = usernameStorage.username else {
            throw ProductNativeApiError.notConnected
        }

        guard try await permissionGuard.consumePermission(
            productId: productId,
            permission: .userIdentityAccess
        ) else {
            throw ProductNativeApiError.permissionDenied
        }

        return GetUserIdResult(primaryUsername: username.value)
    }

    func accountGetAlias(
        context: ProductProofContext,
        ring: RingLocation
    ) async throws -> AccountGetAliasResult {
        let alias = try await aliasHandler.getContextualAlias(context: context, ring: ring)

        return AccountGetAliasResult(context: alias.context, alias: alias.alias)
    }

    func accountCreateProof(
        context: ProductProofContext,
        ring: RingLocation,
        message: Data
    ) async throws -> RingVrfProofResult {
        let proof = try await createProofHandler.createProof(
            context: context,
            ring: ring,
            message: message
        )

        return RingVrfProofResult(
            proof: proof.proof,
            contextualAlias: AccountGetAliasResult(
                context: proof.contextualAlias.context,
                alias: proof.contextualAlias.alias
            ),
            ringIndex: proof.ringIndex,
            ringRevision: proof.ringRevision
        )
    }

    func getNonProductAccounts() async throws -> [LegacyAccountResult] {
        []
    }

    func chainNodes(genesisHash: String) async throws -> [String] {
        let genesis = genesisHash.withoutHexPrefix()

        guard let chain = chainRegistry.getChainByGenesis(for: genesis) else {
            return []
        }

        return chain.nodes.map(\.url)
    }

    func chainSupported(genesisHash: String) async throws -> Bool {
        let genesis = genesisHash.withoutHexPrefix()

        return chainRegistry.getChainByGenesis(for: genesis) != nil
    }
}
