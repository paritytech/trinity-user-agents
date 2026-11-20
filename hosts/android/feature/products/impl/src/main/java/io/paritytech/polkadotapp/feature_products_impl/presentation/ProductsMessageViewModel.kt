package io.paritytech.polkadotapp.feature_products_impl.presentation

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.map
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductMessageRenderObserver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.collections.immutable.persistentSetOf
import kotlinx.collections.immutable.toImmutableSet
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.MaterialFailure
import uniffi.truapi.ResolvedMaterials

/**
 * ViewModel for rendering a single Products message.
 *
 * Takes the message content (scriptId + data bytes) and asynchronously
 * invokes the script executor to produce a widget tree, with the materials its nodes draw.
 */
@HiltViewModel(assistedFactory = ProductsMessageViewModel.Factory::class)
class ProductsMessageViewModel @AssistedInject constructor(
    @Assisted private val content: ProductsMessageContent,
    @Assisted private val messageId: ChatMessageId,
    @Assisted private val worker: ProductWorker,
    @Assisted private val product: Product,
    // Dagger cannot take a value class here: Kotlin mangles the factory method name.
    @Assisted("roomId") private val roomId: String?,
    private val renderObservers: Set<@JvmSuppressWildcards ProductMessageRenderObserver>,
) : BaseViewModel() {
    private val room = roomId?.let(::ProductChatIdParameter)

    private val reports = MutableStateFlow(MaterialReports(resolution = null, nodes = emptySet()))

    private val resolution: StateFlow<MaterialsResolution?> = worker.messageMaterials(room, messageId, content.messageType)
        .stateIn(this, SharingStarted.Eagerly, null)

    private val widget: Flow<LoadingState<JsWidget>> = worker.renderMessage(room, messageId, content.messageType, content.data)
        .map { result ->
            result
                .logFailure("Error receiving render update  for message $messageId in ${product.id}")
                .onSuccess { renderObservers.forEach { it.onMessageRendered(product.id) } }
                .fold(onSuccess = { LoadingState.Loaded(it) }, onFailure = { LoadingState.Error(it) })
        }

    @OptIn(ExperimentalCoroutinesApi::class)
    private val materials: Flow<ProductCardMaterials> = combine(
        resolution.flatMapLatest { current ->
            current?.resolved?.map { resolved -> current to resolved } ?: flowOf(null)
        },
        reports,
    ) { resolved, reported ->
        val current = resolved?.first
        ProductCardMaterials(
            resolution = current,
            resolved = resolved?.second,
            reportedNodes = if (current != null && reported.resolution === current) reported.nodes.toImmutableSet() else persistentSetOf(),
        )
    }

    val state: StateFlow<LoadingState<ProductCard>> = combine(widget, materials) { widgetState, cardMaterials ->
        widgetState.map { ProductCard(widget = it, materials = cardMaterials) }
    }.stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    fun handleUiEvent(actionId: String, eventType: JsUiEvent.Type) {
        val event = JsUiEvent(
            messageId = messageId,
            messageType = content.messageType,
            actionId = actionId,
            eventType = eventType,
            roomId = room,
        )
        worker.dispatchEvent(event)
    }

    fun reportMaterial(drawnFrom: MaterialsResolution, node: Int, material: ByteArray?, reason: MaterialFailure) {
        if (drawnFrom !== resolution.value) return
        val before = reports.getAndUpdate { reported ->
            val nodes = if (reported.resolution === drawnFrom) reported.nodes else emptySet()
            MaterialReports(resolution = drawnFrom, nodes = nodes + node)
        }
        val alreadyReported = before.resolution === drawnFrom && node in before.nodes
        if (!alreadyReported) drawnFrom.report(node.toUInt(), material, reason)
    }

    private class MaterialReports(val resolution: MaterialsResolution?, val nodes: Set<Int>)

    @AssistedFactory
    interface Factory {
        fun create(
            content: ProductsMessageContent,
            messageId: ChatMessageId,
            product: Product,
            worker: ProductWorker,
            @Assisted("roomId") roomId: String?,
        ): ProductsMessageViewModel
    }
}

data class ProductCard(val widget: JsWidget, val materials: ProductCardMaterials)

data class ProductCardMaterials(
    val resolution: MaterialsResolution?,
    val resolved: ResolvedMaterials?,
    val reportedNodes: ImmutableSet<Int>,
)
