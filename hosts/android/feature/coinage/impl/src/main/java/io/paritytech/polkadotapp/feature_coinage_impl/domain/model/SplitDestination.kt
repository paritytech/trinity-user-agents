package io.paritytech.polkadotapp.feature_coinage_impl.domain.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import kotlinx.serialization.Serializable

@AsTuple
@Serializable
data class SplitDestination(
    val exponent: ValueExponent,
    val accountIds: List<AccountId>
)

/**
 * Shape the chain expects for every `split_into` argument: one entry per denomination, carrying all the
 * accounts that receive a coin of that denomination — never a flat list of accounts.
 */
fun List<Coin>.toSplitDestinations(): List<SplitDestination> {
    return groupBy(Coin::valueExponent)
        .entries
        .sortedBy { it.key }
        .map { (exponent, coins) -> SplitDestination(exponent, coins.map(Coin::accountId)) }
}
