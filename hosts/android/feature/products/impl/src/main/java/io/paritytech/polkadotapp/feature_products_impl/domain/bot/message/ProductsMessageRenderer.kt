package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.MessageDrawingContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.productRoomId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.transformLatest
import kotlinx.serialization.KSerializer
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Renderer for custom messages from Products scripts.
 *
 * Draws the widget tree [ProductMessageRenders] holds for each message, and shows a loading
 * indicator until the tree arrives.
 *
 * Where a message is listed rather than drawn, it reads as its alt, then the product's manifest
 * description, then a generic label.
 */
class ProductsMessageRenderer(
    private val appContext: Context,
    private val product: Product,
    private val renders: ProductMessageRenders,
    productImages: ProductImageResolver,
    private val resolveProduct: ResolveProductUseCase,
) : CustomChatMessageRenderer<ProductsMessageContent> {
    override val id: String = product.id.toChatExtensionId()

    val imageResolver = JsImageResolver { source ->
        productImages.resolve(product.id, source)
            .logFailure("ProductsMessageRenderer: card image unavailable for ${product.id.value}")
            .getOrNull()
    }

    override val contentSerializer: KSerializer<ProductsMessageContent> = ProductsMessageContent.serializer()

    @Composable
    override fun DrawMessage(
        message: ChatMessageUiModel.Custom<ProductsMessageContent>,
        context: MessageDrawingContext,
    ) {
        Box(modifier = context.messageModifier) {
            Column(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .fillMaxWidth()
            ) {
                message.content.fold(
                    onSuccess = { content ->
                        ProductsMessageContent(
                            messageId = message.id,
                            content = content,
                            roomId = message.chatId.productRoomId(id),
                        )
                    },
                    onFailure = {
                        NovaText(
                            text = stringResource(RCommon.string.products_message_load_failed),
                            style = PolkadotTheme.typography.body.medium,
                            color = PolkadotTheme.colors.fg.error,
                        )
                    }
                )
            }
        }
    }

    @Composable
    private fun ProductsMessageContent(
        messageId: String,
        content: ProductsMessageContent,
        roomId: ProductChatIdParameter?,
    ) {
        val render = remember(renders, messageId) { renders.render(roomId, messageId, content) }
        val state by render.collectAsState()
        val spinner by remember(render) { render.spinnerAfter(SpinnerDelay) }.collectAsState(initial = false)

        when (val currentState = state) {
            is LoadingState.Loading -> {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = LoadingMinHeight),
                    contentAlignment = Alignment.Center,
                ) {
                    if (spinner) {
                        NovaCircularProgressIndicator(
                            modifier = Modifier.size(24.dp),
                            color = PolkadotTheme.colors.fg.secondary,
                            strokeWidth = 2.dp,
                        )
                    }
                }
            }
            is LoadingState.Loaded -> {
                CompositionLocalProvider(LocalJsImageResolver provides imageResolver) {
                    JsWidgetRenderer(
                        widget = currentState.data,
                        jsEventHandler = { actionId, eventType ->
                            renders.dispatchEvent(roomId, messageId, content, actionId, eventType)
                        },
                    )
                }
            }
            is LoadingState.Error -> {
                NovaText(
                    text = stringResource(RCommon.string.products_message_render_failed),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.error,
                )
            }
        }
    }

    override suspend fun formatNotificationContent(
        message: ChatMessage.Content.Custom<ProductsMessageContent>,
    ): Result<String> {
        return message.content.map {
            it.alt ?: manifestDescription() ?: appContext.getString(RCommon.string.products_message_custom_preview)
        }
    }

    @Composable
    override fun formatChatPreview(
        message: LastMessageUiModel.Custom<ProductsMessageContent>,
    ): Result<String> {
        val description by produceState<String?>(initialValue = null, product.id) { value = manifestDescription() }

        return message.content.map { it.alt ?: description ?: stringResource(RCommon.string.products_message_custom_preview) }
    }

    private suspend fun manifestDescription(): String? = resolveProduct.resolve(product.id).getOrNull()?.description

    private companion object {
        val LoadingMinHeight = 44.dp

        // A tree that arrives within this shows no spinner at all, so a quick load does not flash one.
        val SpinnerDelay = 1.seconds
    }
}

/** Whether a cell still in [LoadingState.Loading] shows its spinner: never before [delay] has passed. */
@OptIn(ExperimentalCoroutinesApi::class)
fun Flow<LoadingState<JsWidget>>.spinnerAfter(delay: Duration): Flow<Boolean> =
    transformLatest { state ->
        emit(false)
        if (state is LoadingState.Loading) {
            delay(delay)
            emit(true)
        }
    }.distinctUntilChanged()
