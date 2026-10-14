package io.paritytech.polkadotapp.feature_products_api.domain.sponsoring

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Sponsoring entry point for Statement Store flows. Owns slot-key lifecycle and
 * pre-submit validation that the sponsorship slot is still present in the on-chain
 * slot table.
 */
interface StatementStoreSubmissionSponsoring {
    /**
     * Ensures a Statement Store allowance slot is allocated for [productId] and returns
     * its slot key. Used when the caller needs to sign a statement proof before submit.
     */
    suspend fun ensureSponsorshipKey(productId: ProductId): Result<SlotAccountKey>

    /**
     * If the statement [signer] matches [productId]'s cached sponsorship slot key,
     * ensures the slot account still holds at least one slot in the on-chain slot table
     * (reallocating with LRU eviction if needed). No-op when [signer] is not a known
     * sponsorship key.
     */
    suspend fun validateSponsorship(productId: ProductId, signer: AccountId): Result<Unit>
}
