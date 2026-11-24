package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

/**
 * Renders the card a product docked in the input slot of one room.
 *
 * Holds one render at a time: showing another card stops the render of the one before. The tree of the card
 * before stays shown until the new card's first tree comes, so the slot crossfades from one card to the next
 * rather than through a spinner.
 */
@HiltViewModel(assistedFactory = ProductRoomFooterCardViewModel.Factory::class)
class ProductRoomFooterCardViewModel @AssistedInject constructor(
    // Dagger cannot take a value class here: Kotlin mangles the factory method name.
    @Assisted("roomId") roomId: String,
    @Assisted private val worker: ProductWorker,
) : BaseViewModel() {
    private val room = ProductChatIdParameter(roomId)
    private var shownCard: ProductRoomFooter.Card? = null
    private var rendering: Job? = null

    private val _state = MutableStateFlow<LoadingState<DockedTree>>(LoadingState.Loading)
    val state: StateFlow<LoadingState<DockedTree>> = _state.asStateFlow()

    fun show(card: ProductRoomFooter.Card) {
        if (card == shownCard) return
        shownCard = card
        rendering?.cancel()
        if (_state.value !is LoadingState.Loaded) _state.value = LoadingState.Loading
        rendering = worker.renderRoomFooter(room, card.messageType, card.payload)
            .onEach { result ->
                result
                    .logFailure("Room footer card render failed in ${room.value}")
                    .onSuccess { _state.value = LoadingState.Loaded(DockedTree(card, it)) }
                    .onFailure { _state.value = LoadingState.Error(it) }
            }
            .launchIn(this)
    }

    fun handleUiEvent(actionId: String, eventType: JsUiEvent.Type) {
        val card = shownCard ?: return
        worker.dispatchRoomFooterEvent(room, card.messageType, actionId, eventType)
    }

    /** A press on the tree of [card]; one on a card the product has since replaced goes nowhere. */
    fun handleUiEvent(card: ProductRoomFooter.Card, actionId: String, eventType: JsUiEvent.Type) {
        if (card == shownCard) handleUiEvent(actionId, eventType)
    }

    /** A tree the product drew for [card]. */
    data class DockedTree(val card: ProductRoomFooter.Card, val tree: JsWidget)

    @AssistedFactory
    interface Factory {
        fun create(@Assisted("roomId") roomId: String, worker: ProductWorker): ProductRoomFooterCardViewModel
    }
}
