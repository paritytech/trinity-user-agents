package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml

import io.novasama.substrate_sdk_android.extensions.requireHexPrefix
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.novasama.substrate_sdk_android.runtime.definitions.types.toHexUntyped
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.chains.util.requireOrml
import io.paritytech.polkadotapp.feature_balances_api.data.type.eventDetector.TokenEventDetector
import io.paritytech.polkadotapp.feature_balances_api.domain.model.DepositEvent
import javax.inject.Inject
import javax.inject.Singleton

class OrmlTokenEventDetector(
    private val runtimeSnapshot: RuntimeSnapshot,
    private val chainAsset: Chain.Asset,
) : TokenEventDetector {
    @Singleton
    class Factory @Inject constructor(
        private val chainRegistry: ChainRegistry,
    ) {
        suspend fun create(chainAsset: Chain.Asset): OrmlTokenEventDetector {
            val runtime = chainRegistry.getRuntime(chainAsset.chainId)
            return OrmlTokenEventDetector(runtime, chainAsset)
        }
    }

    private val ormlType = chainAsset.requireOrml()
    private val targetCurrencyId = ormlType.currencyIdScale.requireHexPrefix()

    override fun detectDeposit(event: GenericEvent.Instance): DepositEvent? {
        return detectTokensDeposited(event)
    }

    private fun detectTokensDeposited(event: GenericEvent.Instance): DepositEvent? {
        if (!event.instanceOf(Modules.TOKENS, "Deposited")) return null

        val (currencyId, who, amount) = event.arguments

        val currencyIdType = runtimeSnapshot.typeRegistry[ormlType.currencyIdType]!!
        val currencyIdEncoded = currencyIdType.toHexUntyped(runtimeSnapshot, currencyId).requireHexPrefix()
        if (currencyIdEncoded != targetCurrencyId) return null

        return DepositEvent(
            destination = bindAccountId(who),
            amount = bindBalance(amount)
        )
    }
}
