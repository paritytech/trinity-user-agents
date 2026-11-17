package io.paritytech.polkadotapp.feature_coinage_impl.data.config

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.belongsTo
import kotlinx.coroutines.CancellationException
import javax.inject.Inject

class ConfiguredInstanceCoins @Inject constructor(
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) {
    class Split(val own: Map<AccountId, OnChainCoinInfo>, val foreign: Set<AccountId>)

    suspend fun splitOrNull(look: Map<AccountId, OnChainCoinInfo?>): Split? {
        val present = look.filterNotNull()
        if (present.isEmpty()) return Split(own = emptyMap(), foreign = emptySet())

        val instance = coinageInstanceIdProvider.instanceId()
            .onFailure { if (it is CancellationException) throw it }
            .logFailure("Can't read the configured coinage instance")
            .getOrNull() ?: return null

        val own = present.filterValues { it.belongsTo(instance) }

        return Split(own, foreign = present.keys - own.keys)
    }
}
