package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.palletNameOrDefault
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.prepareIdForEncoding
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.observeNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.subscribeWithOptionalSharing
import io.paritytech.polkadotapp.chains.util.requireAssets
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api.account
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api.asset
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api.assets
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.AssetsAccount
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.AssetsAssetDetails
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.isBalanceFrozen
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.orEmpty
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.transfersFrozen
import io.paritytech.polkadotapp.feature_balances_impl.domain.model.legacy
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import java.math.BigInteger
import javax.inject.Singleton

internal class AssetsTokenBalanceType @AssistedInject constructor(
    private val chainRegistry: ChainRegistry,
    private val tokenBalanceDao: TokenBalanceDao,
    @RemoteSourceQualifier private val remoteStorage: StorageDataSource,
    @Assisted private val asset: Chain.Asset,
) : TokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): AssetsTokenBalanceType
    }

    private val assetsType = asset.requireAssets()

    override suspend fun minimumBalance(): Balance {
        return remoteStorage.query(asset.chainId) {
            val argument = assetsType.prepareIdForEncoding()
            val palletName = assetsType.palletNameOrDefault()
            metadata.assets(palletName).asset.queryNonNull(argument).minBalance
        }
    }

    override suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*> {
        val chain = chainRegistry.getChain(asset.chainId)
        val accountId = metaAccount.accountIdIn(chain)

        return remoteStorage.subscribe(chain.id, subscriptionBuilder) {
            val assetsType = asset.requireAssets()
            val encodableAssetId = assetsType.prepareIdForEncoding()
            val palletName = assetsType.palletNameOrDefault()

            combine(
                metadata.assets(palletName).asset.observeNonNull(encodableAssetId),
                metadata.assets(palletName).account.observe(encodableAssetId, accountId)
            ) { assetDetails, assetAccount ->
                val tokenBalanceLocal = assetAccount.orEmpty().toLocalTokenBalance(metaAccount.id, assetDetails)

                tokenBalanceDao.insertAsset(tokenBalanceLocal)
            }
        }
    }

    override suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder?
    ): Flow<AccountBalanceUpdate> {
        return remoteStorage.subscribeWithOptionalSharing(asset.chainId, subscriptionBuilder) {
            val encodableId = assetsType.prepareIdForEncoding(runtime)
            val palletName = assetsType.palletNameOrDefault()

            combine(
                metadata.assets(palletName).asset.observeNonNull(encodableId),
                metadata.assets(palletName).account.observeWithRaw(encodableId, accountId)
            ) { assetDetails, assetAccountUpdate ->
                val balance = assetAccountUpdate.value.orEmpty().toTokenBalance(assetDetails)

                AccountBalanceUpdate(assetAccountUpdate.at!!, balance)
            }
        }
    }

    override suspend fun getBalance(accountId: AccountId): TokenBalance {
        return remoteStorage.query(asset.chainId) {
            val encodableId = assetsType.prepareIdForEncoding(runtime)
            val palletName = assetsType.palletNameOrDefault()

            val assetDetails = metadata.assets(palletName).asset.queryNonNull(encodableId)
            val assetAccount = metadata.assets(palletName).account.query(encodableId, accountId)

            assetAccount.orEmpty().toTokenBalance(assetDetails)
        }
    }

    override suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean {
        return true
    }

    override fun isSelfSufficient(): Boolean {
        return assetsType.isSufficient
    }

    private fun AssetsAccount.frozenBalance(assetDetails: AssetsAssetDetails): Balance {
        val fundsFrozen = assetDetails.status.transfersFrozen || isBalanceFrozen

        return if (fundsFrozen) {
            balance
        } else {
            Balance.ZERO
        }
    }

    private fun AssetsAccount.toLocalTokenBalance(
        metaId: Long,
        assetDetails: AssetsAssetDetails
    ): TokenBalanceLocal {
        return TokenBalanceLocal(
            assetId = asset.id,
            chainId = asset.chainId,
            metaId = metaId,
            freeInPlanks = balance.value,
            frozenInPlanks = frozenBalance(assetDetails).value,
            reservedInPlanks = BigInteger.ZERO,
            transferableMode = TokenBalanceLocal.TransferableModeLocal.LEGACY,
            edCountingMode = TokenBalanceLocal.EDCountingModeLocal.TOTAL,
        )
    }

    private fun AssetsAccount.toTokenBalance(assetDetails: AssetsAssetDetails): TokenBalance {
        return TokenBalance.legacy(
            token = asset,
            free = balance,
            reserved = Balance.ZERO,
            frozen = frozenBalance(assetDetails)
        )
    }
}
