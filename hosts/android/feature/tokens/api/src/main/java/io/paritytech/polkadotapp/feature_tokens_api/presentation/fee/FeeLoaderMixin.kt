package io.paritytech.polkadotapp.feature_tokens_api.presentation.fee

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.feature_balances_api.presentation.provider.AvailableBalanceProvider
import io.paritytech.polkadotapp.feature_balances_api.presentation.provider.deduct
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlin.time.Duration.Companion.milliseconds

typealias FeeStatus = LoadingState<FeeModel>

interface FeeLoaderMixin {
    interface Factory {
        fun create(): FeeLoaderMixin
    }

    val status: Flow<FeeStatus>

    suspend fun loadFee(feeConstructor: suspend () -> Result<Fee>)

    suspend fun setFee(fee: Fee)
}

fun FeeLoaderMixin.feeAmount() = status
    .map {
        it.dataOrNull?.displayAmount?.amount
    }
    .filterNotNull()

fun AvailableBalanceProvider.deductFee(feeLoaderMixin: FeeLoaderMixin) =
    deduct(feeLoaderMixin.feeAmount())

context(scope: ComputationalScope)
fun <I> FeeLoaderMixin.connectWith(
    inputSource: Flow<I>,
    feeConstructor: suspend (input: I) -> Result<Fee>,
) {
    inputSource
        .debounce(300.milliseconds)
        .mapLatest {
            loadFee {
                feeConstructor(it)
            }
        }
        .inBackground()
        .launchIn(scope)
}
