package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.deriveAccountId
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.StatementStoreSubmissionSponsoring
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceKeyUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceResourceKind
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import timber.log.Timber
import javax.inject.Inject

class RealStatementStoreSubmissionSponsoring @Inject constructor(
    private val allowanceKeyUseCase: AllowanceKeyUseCase,
    private val statementStoreSlotAllocator: StatementStoreSlotAllocator,
) : StatementStoreSubmissionSponsoring {
    override suspend fun ensureSponsorshipKey(productId: ProductId): Result<SlotAccountKey> {
        return allowanceKeyUseCase.ensure(productId, AllowanceResourceKind.STATEMENT_STORE)
    }

    override suspend fun validateSponsorship(productId: ProductId, signer: AccountId): Result<Unit> {
        val slotKey = allowanceKeyUseCase.getCached(productId, AllowanceResourceKind.STATEMENT_STORE)
        if (slotKey == null) {
            // No key means we have never signed any statement with internal slot account => `statement` cannot be signed by this slot account
            Timber.i("no cached slot key for $productId; skipping sponsorship validation")
            return Result.success(Unit)
        }

        val slotAccount = slotKey.deriveAccountId()
        if (signer != slotAccount) {
            Timber.i("statement signer differs from cached sponsorship key; skipping validation")
            return Result.success(Unit)
        }

        return with(StalenessReportCollector.NoOp) {
            statementStoreSlotAllocator.allocate(slotAccount, OnExistingAllocationStrategy.IGNORE, SlotPriority.Normal)
        }
    }
}
