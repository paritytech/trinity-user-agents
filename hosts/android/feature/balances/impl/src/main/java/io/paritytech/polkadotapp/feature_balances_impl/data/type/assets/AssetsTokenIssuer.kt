package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.instances.AddressInstanceConstructor
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.UntypedAssetsAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.palletNameOrDefault
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.prepareIdForEncoding
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.composeDispatchAs
import io.paritytech.polkadotapp.chains.util.requireAssets
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.issuer.TokenIssuer
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api.asset
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api.assets
import javax.inject.Singleton

class AssetsTokenIssuer @AssistedInject constructor(
    @Assisted chainAsset: Chain.Asset,
    @RemoteSourceQualifier private val remoteStorage: StorageDataSource
) : TokenIssuer {
    private val assetType = chainAsset.requireAssets()
    private val chainId = chainAsset.chainId

    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainAsset: Chain.Asset): AssetsTokenIssuer
    }

    override suspend fun composeIssueCall(amount: Balance, destination: AccountId): GenericCall.Instance {
        return remoteStorage.query(chainId) {
            val encodableAssetId = assetType.prepareIdForEncoding()
            val palletName = assetType.palletNameOrDefault()

            val issuer = metadata.assets(palletName).asset.queryNonNull(encodableAssetId)
                .issuer

            // We're dispatching as issuer since only issuer is allowed to mint tokens
            composeDispatchAs(
                call = composeMint(amount, destination, encodableAssetId),
                origin = OriginCaller.System.Signed(issuer)
            )
        }
    }

    context(withRuntime: WithRuntime)
    private fun composeMint(
        amount: Balance,
        destination: AccountId,
        assetId: UntypedAssetsAssetId
    ): GenericCall.Instance {
        return composeCall(
            moduleName = assetType.palletNameOrDefault(),
            callName = "mint",
            arguments = mapOf(
                "id" to assetId.value,
                "beneficiary" to AddressInstanceConstructor.constructInstance(withRuntime.runtime.typeRegistry, destination.value),
                "amount" to amount.value
            )
        )
    }
}
