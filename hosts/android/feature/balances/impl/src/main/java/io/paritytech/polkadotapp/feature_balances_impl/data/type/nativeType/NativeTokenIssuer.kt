package io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.instances.AddressInstanceConstructor
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.issuer.TokenIssuer
import javax.inject.Singleton

class NativeTokenIssuer @AssistedInject constructor(
    @Assisted private val chainAsset: Chain.Asset,
    private val chainRegistry: ChainRegistry
) : TokenIssuer {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainAsset: Chain.Asset): NativeTokenIssuer
    }

    override suspend fun composeIssueCall(amount: Balance, destination: AccountId): GenericCall.Instance {
        val runtime = chainRegistry.getRuntime(chainAsset.chainId)

        return runtime.composeCall(
            moduleName = Modules.BALANCES,
            callName = "force_set_balance",
            arguments = mapOf(
                "who" to AddressInstanceConstructor.constructInstance(runtime.typeRegistry, destination.value),
                "new_free" to amount.value
            )
        )
    }
}
