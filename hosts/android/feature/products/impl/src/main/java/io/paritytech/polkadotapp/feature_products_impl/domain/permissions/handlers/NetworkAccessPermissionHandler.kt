package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission.NetworkAccess
import java.net.URI
import javax.inject.Inject

class NetworkAccessPermissionHandler @Inject constructor(
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
) : ProductPermissionHandler<NetworkAccess> {
    private val allowedDomains = setOf(
        "fonts.googleapis.com",
        "fonts.gstatic.com",
        "paseo-bulletin-next-ipfs.polkadot.io"
    )

    override suspend fun isGranted(productId: ProductId, permission: NetworkAccess): Boolean {
        if (permission.domain in allowedDomains) return true

        val candidates = generateDomainCandidates(permission.domain)
        return repository.isAnyGranted(productId, permission.typeName, candidates)
    }

    override suspend fun request(productId: ProductId, permission: NetworkAccess): Boolean {
        if (isGranted(productId, permission)) return true

        return when (requester.prompt(productId, permission)) {
            PermissionDecision.AllowAlways -> {
                repository.grant(productId, permission)
                true
            }
            PermissionDecision.AllowOnce -> {
                repository.grantOneTime(productId, permission)
                true
            }
            PermissionDecision.Deny -> false
        }
    }

    override suspend fun revoke(productId: ProductId, permission: NetworkAccess) {
        repository.revoke(productId, permission)
    }

    companion object {
        fun extractDomain(url: String): String? {
            return try {
                URI(url).host?.lowercase()
            } catch (_: Exception) {
                null
            }
        }

        /**
         * Generates all stored-pattern candidates that would grant access to [domain].
         *
         * Includes:
         *  - the domain itself (exact match),
         *  - each parent domain with a leading `*.` (explicit wildcard match per v0.2 spec),
         *  - `*` (universal wildcard).
         *
         * Bare parent domains are NOT included: access to `a.example.com` must be granted
         * explicitly either for `a.example.com` or via the wildcard `*.example.com`. A grant
         * for `example.com` alone does not imply access to its subdomains.
         *
         * Stops at second-level domains to avoid matching TLDs.
         */
        fun generateDomainCandidates(domain: String): List<String> {
            val candidates = mutableListOf<String>()
            candidates.add(domain)

            val parts = domain.split('.')
            for (i in 1..parts.size - 2) {
                val parent = parts.drop(i).joinToString(".")
                candidates.add("*.$parent")
            }
            candidates.add("*")
            return candidates
        }
    }
}
