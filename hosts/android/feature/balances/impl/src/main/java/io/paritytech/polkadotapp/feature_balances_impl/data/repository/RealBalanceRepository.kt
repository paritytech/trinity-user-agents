package io.paritytech.polkadotapp.feature_balances_impl.data.repository

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.memory.useSharedFlow
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal.EDCountingModeLocal
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal.TransferableModeLocal
import io.paritytech.polkadotapp.feature_balances_api.data.repository.BalanceRepository
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHold
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.balances
import io.paritytech.polkadotapp.feature_balances_impl.data.holds
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.EDCountingMode
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.RealTokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.TransferableMode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.collections.orEmpty

@Singleton
internal class RealBalanceRepository @Inject constructor(
    private val tokenBalanceDao: TokenBalanceDao,
    private val computationalCache: ComputationalCache,
    @RemoteSourceQualifier private val remoteStorageDataSource: StorageDataSource
) : BalanceRepository {
    override fun syncedTokenBalanceFlow(metaId: Long, chainAsset: Chain.Asset): Flow<TokenBalance> {
        return tokenBalanceDao.observeAsset(metaId, chainAsset.chainId, chainAsset.id)
            .filterNotNull()
            .map { it.toDomain(chainAsset) }
    }

    override suspend fun getSyncedTokenBalance(metaId: Long, chainAsset: Chain.Asset): TokenBalance {
        return syncedTokenBalanceFlow(metaId, chainAsset)
            .inBackground()
            .first()
    }

    context(scope: ComputationalScope)
    override fun observeBalanceHolds(chainId: ChainId, accountId: AccountId): Flow<List<BalanceHold>> {
        return computationalCache.useSharedFlow("BalanceHolds", chainId, accountId.value.toHexString()) {
            remoteStorageDataSource.subscribe(chainId) {
                metadata.balances.holds.observe(accountId.value).map { it.orEmpty() }
            }
        }
    }

    private fun TokenBalanceLocal.toDomain(chainAsset: Chain.Asset): TokenBalance {
        return RealTokenBalance(
            token = chainAsset,
            free = freeInPlanks.intoBalance(),
            reserved = reservedInPlanks.intoBalance(),
            frozen = frozenInPlanks.intoBalance(),
            edCountingMode = edCountingMode.toDomain(),
            transferableMode = transferableMode.toDomain()
        )
    }

    private fun EDCountingModeLocal.toDomain(): EDCountingMode {
        return when (this) {
            EDCountingModeLocal.TOTAL -> EDCountingMode.TOTAL
            EDCountingModeLocal.FREE -> EDCountingMode.FREE
        }
    }

    private fun TransferableModeLocal.toDomain(): TransferableMode {
        return when (this) {
            TransferableModeLocal.LEGACY -> TransferableMode.LEGACY
            TransferableModeLocal.HOLDS_AND_FREEZES -> TransferableMode.HOLDS_AND_FREEZES
        }
    }
}
