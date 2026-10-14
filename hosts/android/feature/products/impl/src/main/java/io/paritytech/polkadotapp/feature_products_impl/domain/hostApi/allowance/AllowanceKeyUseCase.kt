package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance

import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatedResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.requestResourceAllocation
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.AllowanceDeniedException
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.AllowanceUnavailableException
import javax.inject.Inject

/**
 * Single entry point for slot-key bookkeeping. Sponsoring impls and the explicit
 * allocation flow funnel through here so [AllowanceKeyStorage] is not touched directly.
 */
class AllowanceKeyUseCase @Inject constructor(
    private val allowanceKeyStorage: AllowanceKeyStorage,
    private val accountsProtocol: AccountsProtocol,
) {
    suspend fun getCached(productId: ProductId, kind: AllowanceResourceKind): SlotAccountKey? {
        return allowanceKeyStorage.get(productId, kind)
    }

    /**
     * Returns the cached slot key for [kind], or allocates one with `IGNORE` policy and
     * caches it before returning. Failure surfaces user rejection / unavailable resource.
     */
    suspend fun ensure(productId: ProductId, kind: AllowanceResourceKind): Result<SlotAccountKey> = runCatching {
        allowanceKeyStorage.get(productId, kind)?.let { return@runCatching it }

        val outcome = accountsProtocol.requestResourceAllocation(
            callingProduct = productId,
            resource = kind.toApResource(),
            onExisting = OnExistingAllowancePolicy.IGNORE,
        )
        when (outcome) {
            is ApAllocationOutcome.Allocated -> {
                val key = outcome.resource.slotKey()
                    ?: error("Allocator returned non-slot resource for $kind")
                allowanceKeyStorage.put(productId, kind, key)
                key
            }
            ApAllocationOutcome.Rejected -> throw AllowanceDeniedException(kind)
            ApAllocationOutcome.NotAvailable -> throw AllowanceUnavailableException(kind)
        }
    }

    suspend fun persistIfAllocated(productId: ProductId, outcome: ApAllocationOutcome) {
        val resource = (outcome as? ApAllocationOutcome.Allocated)?.resource ?: return
        val kind = resource.toAllowanceResourceKind() ?: return
        val key = resource.slotKey() ?: return
        allowanceKeyStorage.put(productId, kind, key)
    }
}

private fun AllowanceResourceKind.toApResource(): ApAllocatableResource = when (this) {
    AllowanceResourceKind.BULLETIN -> ApAllocatableResource.BulletInAllowance
    AllowanceResourceKind.STATEMENT_STORE -> ApAllocatableResource.StatementStoreAllowance
}

private fun ApAllocatedResource.slotKey(): SlotAccountKey? = when (this) {
    is ApAllocatedResource.BulletInAllowance -> slotAccountKey
    is ApAllocatedResource.StatementStoreAllowance -> slotAccountKey
    ApAllocatedResource.SmartContractAllowance,
    is ApAllocatedResource.AutoSigning -> null
}

private fun ApAllocatedResource.toAllowanceResourceKind(): AllowanceResourceKind? = when (this) {
    is ApAllocatedResource.BulletInAllowance -> AllowanceResourceKind.BULLETIN
    is ApAllocatedResource.StatementStoreAllowance -> AllowanceResourceKind.STATEMENT_STORE
    ApAllocatedResource.SmartContractAllowance,
    is ApAllocatedResource.AutoSigning -> null
}
