package io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.asLoaded
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpAcknowledgement
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpRequestContextHolder
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

@HiltViewModel
class TopUpRequestViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val context: TopUpRequestContext,
    private val holder: TopUpRequestContextHolder,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val tokenAmountMapper: TokenAmountMapper,
) : BaseViewModel() {
    val state: StateFlow<LoadingState<TopUpRequestUiState>>
        field = MutableStateFlow<LoadingState<TopUpRequestUiState>>(LoadingState.Loading)

    init {
        launchUnit {
            state.value = mapAcknowledgement(context.acknowledgement).asLoaded()
        }
    }

    fun onDismissClicked() = launchUnit {
        context.deliverDismissed()
        router.back()
    }

    override fun onCleared() {
        super.onCleared()
        holder.clear()
    }

    private suspend fun mapAcknowledgement(acknowledgement: TopUpAcknowledgement): TopUpRequestUiState =
        when (acknowledgement) {
            is TopUpAcknowledgement.Failure -> TopUpRequestUiState.Failure(acknowledgement.productId.value)

            is TopUpAcknowledgement.PartialPayment -> {
                val asset = chainAssetProvider.asset()
                TopUpRequestUiState.PartialPayment(
                    productId = acknowledgement.productId.value,
                    requestedAmount = tokenAmountMapper.mapFrom(asset.withAmount(acknowledgement.requested)),
                    creditedAmount = tokenAmountMapper.mapFrom(asset.withAmount(acknowledgement.credited)),
                )
            }
        }
}
