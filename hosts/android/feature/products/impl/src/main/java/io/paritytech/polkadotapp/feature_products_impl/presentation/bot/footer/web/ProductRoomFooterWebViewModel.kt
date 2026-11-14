package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductServingHostResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.frameEmbeddingResponseHeaders
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import uniffi.truapi.ExecutableKind

private const val ACTION_DEBOUNCE_MILLIS = 500L

/**
 * Backs the page a product docked in the input slot of one room: serves the product's own content to it and delivers
 * the presses it reports to the product.
 */
@HiltViewModel(assistedFactory = ProductRoomFooterWebViewModel.Factory::class)
class ProductRoomFooterWebViewModel @AssistedInject constructor(
    // Dagger cannot take a value class here: Kotlin mangles the factory method name.
    @Assisted("roomId") roomId: String,
    @Assisted private val worker: ProductWorker,
    dotNsResolver: DotNsResolver,
    dotNsTldProvider: DotNsTldProvider,
    servingHostResolver: ProductServingHostResolver,
) : BaseViewModel() {
    private val room = ProductChatIdParameter(roomId)
    private val debounce = ProductFooterActionDebounce(ACTION_DEBOUNCE_MILLIS) { System.nanoTime() / 1_000_000 }
    private val _hostEvents = Channel<ProductFooterHostEvent>(Channel.BUFFERED)

    /** What the page is told after each press it reported. */
    val hostEvents: Flow<ProductFooterHostEvent> = _hostEvents.receiveAsFlow()

    /** Serves the product's App archive, the content its pages load from, with iframes refused. */
    val content: DotNsWebViewClient = DotNsWebViewClient(
        dotNsResolver,
        dotNsTldProvider,
        DotNsServingHostResolver { host -> servingHostResolver.servingHostFor(host, ExecutableKind.APP) },
        frameEmbeddingResponseHeaders(allowIframes = false),
    )

    fun onPageAction(action: ProductFooterMessage.Action) {
        if (!debounce.tryAccept()) return
        val delivered = worker.dispatchRoomFooterPageAction(room, action.actionId, action.payload)
        _hostEvents.trySend(if (delivered) ProductFooterHostEvent.Confirmed else ProductFooterHostEvent.Reset)
    }

    @AssistedFactory
    interface Factory {
        fun create(@Assisted("roomId") roomId: String, worker: ProductWorker): ProductRoomFooterWebViewModel
    }
}
