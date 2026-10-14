package io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.TokenEventDetector
import io.paritytech.polkadotapp.feature_balances_api.domain.model.DepositEvent
import javax.inject.Singleton

class NativeTokenEventDetector @AssistedInject constructor(
    @Assisted chainAsset: Chain.Asset
) : TokenEventDetector {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainAsset: Chain.Asset): NativeTokenEventDetector
    }

    override fun detectDeposit(event: GenericEvent.Instance): DepositEvent? {
        return detectMinted(event)
            ?: detectBalancesDeposit(event)
    }

    private fun detectMinted(event: GenericEvent.Instance): DepositEvent? {
        if (!event.instanceOf(Modules.BALANCES, "Minted")) return null

        val (who, amount) = event.arguments

        return DepositEvent(
            destination = bindAccountId(who),
            amount = bindBalance(amount)
        )
    }

    private fun detectBalancesDeposit(event: GenericEvent.Instance): DepositEvent? {
        if (!event.instanceOf(Modules.BALANCES, "Deposit")) return null

        val (who, amount) = event.arguments

        return DepositEvent(
            destination = bindAccountId(who),
            amount = bindBalance(amount)
        )
    }
}
