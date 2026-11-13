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
 * Holds one render at a time: showing another card stops the render of the one before.
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

    private val _state = MutableStateFlow<LoadingState<JsWidget>>(LoadingState.Loading)
    val state: StateFlow<LoadingState<JsWidget>> = _state.asStateFlow()

    fun show(card: ProductRoomFooter.Card) {
        if (card == shownCard) return
        shownCard = card
        rendering?.cancel()
        _state.value = LoadingState.Loading
        rendering = worker.renderRoomFooter(room, card.messageType, card.payload)
            .onEach { result ->
                result
                    .logFailure("Room footer card render failed in ${room.value}")
                    .onSuccess { _state.value = LoadingState.Loaded(it) }
                    .onFailure { _state.value = LoadingState.Error(it) }
            }
            .launchIn(this)
    }

    fun handleUiEvent(actionId: String, eventType: JsUiEvent.Type) {
        val card = shownCard ?: return
        worker.dispatchRoomFooterEvent(room, card.messageType, actionId, eventType)
    }

    @AssistedFactory
    interface Factory {
        fun create(@Assisted("roomId") roomId: String, worker: ProductWorker): ProductRoomFooterCardViewModel
    }
}
