package io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.AccountInfo
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.canDecrementProvider
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.network.binding.orEmpty
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.subscribeWithOptionalSharing
import io.paritytech.polkadotapp.chains.storage.typed.account
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.chains.util.balances
import io.paritytech.polkadotapp.chains.util.numberConstant
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.EDCountingMode
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.RealTokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.TransferableMode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import javax.inject.Singleton

internal class NativeTokenBalanceType @AssistedInject constructor(
    @RemoteSourceQualifier private val remoteStorageDataSource: StorageDataSource,
    private val tokenBalanceDao: TokenBalanceDao,
    private val chainRegistry: ChainRegistry,
    @Assisted private val asset: Chain.Asset,
) : TokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): NativeTokenBalanceType
    }

    override suspend fun minimumBalance(): Balance {
        return chainRegistry.withRuntime(asset.chainId) {
            runtime.metadata.balances().numberConstant("ExistentialDeposit").intoBalance()
        }
    }

    override suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*> {
        val chain = chainRegistry.getChain(asset.chainId)
        return remoteStorageDataSource.subscribe(chain.id, subscriptionBuilder) {
            metadata.system.account.observe(metaAccount.accountIdIn(chain))
        }.onEach { accountInfo ->
            val tokenBalanceLocal = accountInfo.orEmpty().toLocalTokenBalance(metaAccount.id)

            tokenBalanceDao.insertAsset(tokenBalanceLocal)
        }
    }

    override suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder?
    ): Flow<AccountBalanceUpdate> {
        val chain = chainRegistry.getChain(asset.chainId)
        return remoteStorageDataSource.subscribeWithOptionalSharing(chain.id, subscriptionBuilder) {
            metadata.system.account.observeWithRaw(accountId).map {
                val balance = it.value.orEmpty().toTokenBalance()
                AccountBalanceUpdate(it.at!!, balance)
            }
        }
    }

    override suspend fun getBalance(accountId: AccountId): TokenBalance {
        return remoteStorageDataSource.query(asset.chainId) {
            metadata.system.account.query(accountId).orEmpty().toTokenBalance()
        }
    }

    override suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean {
        return remoteStorageDataSource.query(asset.chainId) {
            val account = metadata.system.account.query(accountId)

            account != null && account.canDecrementProvider()
        }
    }

    override fun isSelfSufficient(): Boolean {
        return true
    }

    private fun AccountInfo.toTokenBalance(): TokenBalance {
        val transferableMode: TransferableMode
        val edCountingMode: EDCountingMode

        if (data.flags.holdsAndFreezesEnabled()) {
            transferableMode = TransferableMode.HOLDS_AND_FREEZES
            edCountingMode = EDCountingMode.FREE
        } else {
            transferableMode = TransferableMode.LEGACY
            edCountingMode = EDCountingMode.TOTAL
        }

        return RealTokenBalance(
            token = asset,
            free = data.free,
            reserved = data.reserved,
            frozen = data.frozen,
            edCountingMode = edCountingMode,
            transferableMode = transferableMode
        )
    }

    private fun AccountInfo.toLocalTokenBalance(metaId: Long): TokenBalanceLocal {
        val transferableMode: TokenBalanceLocal.TransferableModeLocal
        val edCountingMode: TokenBalanceLocal.EDCountingModeLocal

        val balanceData = data

        if (balanceData.flags.holdsAndFreezesEnabled()) {
            transferableMode = TokenBalanceLocal.TransferableModeLocal.HOLDS_AND_FREEZES
            edCountingMode = TokenBalanceLocal.EDCountingModeLocal.FREE
        } else {
            transferableMode = TokenBalanceLocal.TransferableModeLocal.LEGACY
            edCountingMode = TokenBalanceLocal.EDCountingModeLocal.TOTAL
        }

        return TokenBalanceLocal(
            assetId = asset.id,
            chainId = asset.chainId,
            metaId = metaId,
            freeInPlanks = balanceData.free.value,
            frozenInPlanks = balanceData.frozen.value,
            reservedInPlanks = balanceData.reserved.value,
            transferableMode = transferableMode,
            edCountingMode = edCountingMode
        )
    }
}
