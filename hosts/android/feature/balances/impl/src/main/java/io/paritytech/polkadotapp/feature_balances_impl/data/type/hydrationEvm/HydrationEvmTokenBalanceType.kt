package io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.requireOrml
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.OrmlTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.orEmpty
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.toLocalTokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.toTokenBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import javax.inject.Singleton

internal class HydrationEvmTokenBalanceType @AssistedInject constructor(
    delegateFactory: OrmlTokenBalanceType.Factory,
    private val chainRegistry: ChainRegistry,
    private val tokenBalanceDao: TokenBalanceDao,
    private val hydrationEvmBalancePoller: HydrationEvmBalancePoller,
    @Assisted private val asset: Chain.Asset,
) : TokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): HydrationEvmTokenBalanceType
    }

    private val delegate = delegateFactory.create(asset)

    private val ormlType = asset.requireOrml()

    override suspend fun minimumBalance(): Balance {
        return delegate.minimumBalance()
    }

    override suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*> {
        val chain = chainRegistry.getChain(asset.chainId)
        val accountId = metaAccount.accountIdIn(chain)

        val runtime = chainRegistry.getRuntime(asset.chainId)
        val assetId = ormlType.currencyId(runtime)

        return hydrationEvmBalancePoller.pollBalanceFlow(
            chainId = chain.id,
            accountId = accountId,
            assetId = assetId,
            subscriptionBuilder = subscriptionBuilder,
        )
            .map { it.balance }
            .onEach { assetAccount ->
                val tokenBalanceLocal = assetAccount.orEmpty()
                    .toLocalTokenBalance(asset, metaAccount.id)
                tokenBalanceDao.insertAsset(tokenBalanceLocal)
            }
    }

    override suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder?
    ): Flow<AccountBalanceUpdate> {
        val runtime = chainRegistry.getRuntime(asset.chainId)
        val assetId = ormlType.currencyId(runtime)

        return hydrationEvmBalancePoller.pollBalanceFlow(
            chainId = asset.chainId,
            accountId = accountId,
            assetId = assetId,
        )
            .mapNotNull {
                val balance = it.balance.toTokenBalance(asset)
                AccountBalanceUpdate(it.at, balance)
            }
    }

    override suspend fun getBalance(accountId: AccountId): TokenBalance {
        val runtime = chainRegistry.getRuntime(asset.chainId)
        val assetId = ormlType.currencyId(runtime)
        val ormlAccount = hydrationEvmBalancePoller.fetchBalance(
            chainId = asset.chainId,
            accountId = accountId,
            assetId = assetId,
        )
        return ormlAccount.toTokenBalance(asset)
    }

    override suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean {
        return true
    }

    override fun isSelfSufficient(): Boolean {
        return delegate.isSelfSufficient()
    }
}
