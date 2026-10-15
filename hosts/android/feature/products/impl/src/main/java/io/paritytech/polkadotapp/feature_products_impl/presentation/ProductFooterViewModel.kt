package io.paritytech.polkadotapp.feature_products_impl.presentation

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
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

/** The tree of the footer that the product draws for one room (`RenderContext::ChatFooter`, 02-DESIGN §6.4). */
@HiltViewModel(assistedFactory = ProductFooterViewModel.Factory::class)
class ProductFooterViewModel @AssistedInject constructor(
    @Assisted private val worker: ProductWorker,
    // Dagger cannot take a value class here: Kotlin mangles the factory method name.
    @Assisted("roomId") roomId: String,
) : BaseViewModel() {
    private val room = ProductChatIdParameter(roomId)

    private val _state = MutableStateFlow<LoadingState<JsWidget>>(LoadingState.Loading)
    val state: StateFlow<LoadingState<JsWidget>> = _state.asStateFlow()

    init {
        worker.renderFooter(room)
            .onEach { result ->
                result.logFailure("The footer of room ${room.value} did not draw")
                    .onSuccess { _state.value = LoadingState.Loaded(it) }
                    .onFailure { _state.value = LoadingState.Error(it) }
            }
            .launchIn(this)
    }

    fun handleUiEvent(actionId: String, eventType: JsUiEvent.Type) {
        worker.dispatchFooterEvent(room, actionId, eventType)
    }

    @AssistedFactory
    interface Factory {
        fun create(worker: ProductWorker, @Assisted("roomId") roomId: String): ProductFooterViewModel
    }
}
