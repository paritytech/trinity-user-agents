package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.pocketAddCard.PocketAddCardInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.pocketAddCard.PocketAddCardOffer
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class PocketAddCardViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val interactor: PocketAddCardInteractor,
    private val router: ProductsRouter,
) : BaseViewModel(), PocketAddCardContract {
    private val payload = savedStateHandle.getPayload<PocketAddCardPayload>()
    private val productId = ProductId.fromStoredValue(payload.productId)

    private val adding = MutableStateFlow(false)

    // The sheet promises the user this is how the card will look, so its images have to be the
    // product's own. One per screen: the renderer resolves again whenever the resolver changes.
    private val imageResolver = JsImageResolver { source ->
        interactor.resolveFaceImage(productId, source)
            .logFailure("PocketAddCard: face image unavailable")
            .getOrNull()
    }

    // Loaded once: the face the user approves must be the one that is stored.
    private val offer: StateFlow<Result<PocketAddCardOffer>?> = flowOf {
        interactor.loadOffer(productId, PocketCardId(payload.cardId))
    }
        .stateIn(this, SharingStarted.Eagerly, null)

    override val state: StateFlow<LoadingState<PocketAddCardUiState>> =
        combine(offer.filterNotNull(), adding) { offerResult, isAdding ->
            offerResult.map { it.toUiState(isAdding) }
        }
            .withLoading("PocketAddCard: failed to load the published card")
            .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onAddClicked() = launchUnit {
        if (!adding.compareAndSet(expect = false, update = true)) return@launchUnit

        val loaded = offer.value?.getOrNull() ?: run {
            adding.value = false
            return@launchUnit
        }
        interactor.approve(loaded)
            .logFailure("PocketAddCard: failed to add the card")
            .onSuccess { router.back() }
            .onFailure {
                adding.value = false
                showMessage(RCommon.string.pocket_add_card_failed)
            }
    }

    override fun onCancelClicked() {
        router.back()
    }

    private fun PocketAddCardOffer.toUiState(adding: Boolean) = PocketAddCardUiState(
        productName = productName,
        title = title,
        face = face.toJsWidget(),
        imageResolver = imageResolver,
        adding = adding,
    )
}
