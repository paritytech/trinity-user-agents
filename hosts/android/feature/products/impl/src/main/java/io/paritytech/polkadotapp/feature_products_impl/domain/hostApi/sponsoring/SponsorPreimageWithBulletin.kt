package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.deriveAccountId
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.requestResourceAllocation
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.PreimageSubmitSponsoring
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceKeyUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceResourceKind
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageRepository
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorization
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.hasCapacityFor
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.hasExpiredAt
import timber.log.Timber
import javax.inject.Inject

class SponsorPreimageWithBulletin @Inject constructor(
    private val knownChains: KnownChains,
    private val allowanceKeyUseCase: AllowanceKeyUseCase,
    private val transactionStorageRepository: TransactionStorageRepository,
    private val chainStateRepository: ChainStateRepository,
    private val accountsProtocol: AccountsProtocol,
) : PreimageSubmitSponsoring {
    override suspend fun sponsorPreimageSubmit(productId: ProductId, size: InformationSize): Result<SlotAccountKey> {
        return allowanceKeyUseCase.ensure(productId, AllowanceResourceKind.BULLETIN).flatMap { slotKey ->
            val bulletInChainId = knownChains.bulletIn
            val accountId = slotKey.deriveAccountId()

            transactionStorageRepository.getAuthorization(bulletInChainId, accountId, CacheableDataConsistency.CONSISTENT_WITH_REMOTE).mapCatching { authorization ->
                val currentBlock = chainStateRepository.currentBlock(bulletInChainId)
                if (!authorization.needsExtension(currentBlock, size)) {
                    Timber.i("authorization sufficient for ${size.inWholeBytes} bytes")
                    return@mapCatching slotKey
                }

                Timber.i("extending allowance for $productId (size=${size.inWholeBytes}B)")
                val outcome = accountsProtocol.requestResourceAllocation(
                    callingProduct = productId,
                    resource = ApAllocatableResource.BulletInAllowance,
                    onExisting = OnExistingAllowancePolicy.INCREASE,
                )
                when (outcome) {
                    is ApAllocationOutcome.Allocated -> slotKey
                    ApAllocationOutcome.Rejected -> error("Bulletin allocation rejected by user")
                    ApAllocationOutcome.NotAvailable -> error("Bulletin allocation unavailable")
                }
            }
        }
    }

    private fun TransactionStorageAuthorization?.needsExtension(currentBlock: BlockNumber, size: InformationSize): Boolean {
        return when {
            this == null -> true
            hasExpiredAt(currentBlock) -> true
            !hasCapacityFor(size) -> true
            else -> false
        }
    }
}
