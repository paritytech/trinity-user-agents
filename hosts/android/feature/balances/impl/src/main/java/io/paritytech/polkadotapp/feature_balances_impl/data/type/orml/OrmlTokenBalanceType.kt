package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.subscribeWithOptionalSharing
import io.paritytech.polkadotapp.chains.util.requireOrml
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.api.accounts
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.api.tokens
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.orEmpty
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.toLocalTokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.toTokenBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Singleton

internal class OrmlTokenBalanceType @AssistedInject constructor(
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
    private val tokenBalanceDao: TokenBalanceDao,
    @Assisted private val asset: Chain.Asset,
) : TokenBalanceType {
    private val ormlType = asset.requireOrml()

    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): OrmlTokenBalanceType
    }

    override suspend fun minimumBalance(): Balance {
        return ormlType.existentialDeposit.intoBalance()
    }

    override suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*> {
        val chain = chainRegistry.getChain(asset.chainId)
        val accountId = metaAccount.accountIdIn(chain)

        return remoteStorageSource.subscribe(asset.chainId, subscriptionBuilder) {
            val currencyId = ormlType.currencyId()
            metadata.tokens.accounts.observe(accountId, currencyId)
        }.map { assetAccount ->
            val tokenBalanceLocal = assetAccount.orEmpty()
                .toLocalTokenBalance(asset, metaAccount.id)
            tokenBalanceDao.insertAsset(tokenBalanceLocal)
        }
    }

    override suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder?
    ): Flow<AccountBalanceUpdate> {
        return remoteStorageSource.subscribeWithOptionalSharing(asset.chainId, subscriptionBuilder) {
            val currencyId = ormlType.currencyId(runtime)

            metadata.tokens.accounts.observeWithRaw(accountId, currencyId)
                .map {
                    val balance = it.value.orEmpty().toTokenBalance(asset)
                    AccountBalanceUpdate(it.at!!, balance)
                }
        }
    }

    override suspend fun getBalance(accountId: AccountId): TokenBalance {
        return remoteStorageSource.query(asset.chainId) {
            val currencyId = ormlType.currencyId(runtime)
            metadata.tokens.accounts.query(accountId, currencyId).orEmpty().toTokenBalance(asset)
        }
    }

    override suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean {
        return true
    }

    override fun isSelfSufficient(): Boolean {
        return true
    }
}
