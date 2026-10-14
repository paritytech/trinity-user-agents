package io.paritytech.polkadotapp.feature_tokens_impl.presentation.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.singleReplaySharedFlow
import io.paritytech.polkadotapp.feature_tokens_api.presentation.fee.FeeLoaderMixin
import io.paritytech.polkadotapp.feature_tokens_api.presentation.fee.FeeModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.fee.FeeStatus
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import kotlinx.coroutines.CancellationException
import timber.log.Timber
import javax.inject.Inject

class RealFeeLoaderMixin(
    private val tokenAmountMapper: TokenAmountMapper,
) : FeeLoaderMixin {
    override val status = singleReplaySharedFlow<FeeStatus>()

    override suspend fun loadFee(feeConstructor: suspend () -> Result<Fee>) {
        setFeeLoading()
        status.emit(
            feeConstructor().fold(
                onSuccess = { fee -> fee.toStatus() },
                onFailure = { exception -> onError(exception) })
        )
    }

    override suspend fun setFee(fee: Fee) {
        status.emit(fee.toStatus())
    }

    private suspend fun setFeeLoading() {
        status.emit(LoadingState.Loading)
    }

    private fun Fee.toStatus(): FeeStatus {
        val amountModel = tokenAmountMapper.mapFrom(asset.withAmount(amount))
        val model = FeeModel(this, amountModel)
        return LoadingState.Loaded(model)
    }

    private fun onError(exception: Throwable): FeeStatus {
        return if (exception !is CancellationException) {
            Timber.e(exception)

            LoadingState.Error(exception)
        } else {
            LoadingState.Loading
        }
    }
}

class FeeLoaderMixinFactory @Inject constructor(
    private val tokenAmountMapper: TokenAmountMapper,
) : FeeLoaderMixin.Factory {
    override fun create(): FeeLoaderMixin {
        return RealFeeLoaderMixin(tokenAmountMapper)
    }
}
