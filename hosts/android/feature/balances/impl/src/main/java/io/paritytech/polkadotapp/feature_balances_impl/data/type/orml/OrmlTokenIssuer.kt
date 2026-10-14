package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.instances.AddressInstanceConstructor
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.requireOrml
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.issuer.TokenIssuer
import javax.inject.Singleton

class OrmlTokenIssuer @AssistedInject constructor(
    @Assisted private val asset: Chain.Asset,
    private val chainRegistry: ChainRegistry
) : TokenIssuer {
    private val ormlType = asset.requireOrml()

    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainAsset: Chain.Asset): OrmlTokenIssuer
    }

    override suspend fun composeIssueCall(amount: Balance, destination: AccountId): GenericCall.Instance {
        val runtime = chainRegistry.getRuntime(asset.chainId)

        return runtime.composeCall(
            moduleName = Modules.TOKENS,
            callName = "set_balance",
            arguments = mapOf(
                "who" to AddressInstanceConstructor.constructInstance(runtime.typeRegistry, destination.value),
                "currency_id" to ormlType.currencyId(runtime).value,
                "new_free" to amount.value,
                "new_reserved" to Balance.ZERO.value
            )
        )
    }
}
