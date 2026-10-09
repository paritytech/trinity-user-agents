package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.MessageDrawingContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.productRoomId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.presentation.ProductsMessageViewModel
import kotlinx.serialization.KSerializer
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Renderer for custom messages from Products scripts.
 *
 * Uses [ProductsMessageViewModel] to asynchronously invoke the script executor
 * and render the widget tree.
 */
class ProductsMessageRenderer(
    private val appContext: Context,
    private val product: Product,
    private val worker: ProductWorker,
) : CustomChatMessageRenderer<ProductsMessageContent> {
    override val id: String = product.id.toChatExtensionId()

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
                    .widthIn(max = 280.dp)
            ) {
                message.content.fold(
                    onSuccess = { content ->
                        ProductsMessageContent(
                            messageId = message.id,
                            content = content,
                            roomId = message.chatId.productRoomId(id)?.value,
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
        roomId: String?,
    ) {
        val viewModel: ProductsMessageViewModel = hiltViewModel(
            key = messageId,
            creationCallback = { factory: ProductsMessageViewModel.Factory ->
                factory.create(content, messageId, product, worker, roomId)
            }
        )

        val state by viewModel.state.collectAsState()

        when (val currentState = state) {
            is LoadingState.Loading -> {
                CircularProgressIndicator(
                    modifier = Modifier.size(24.dp),
                    color = PolkadotTheme.colors.fg.secondary,
                    strokeWidth = 2.dp,
                )
            }
            is LoadingState.Loaded -> {
                JsWidgetRenderer(
                    widget = currentState.data,
                    jsEventHandler = viewModel::handleUiEvent,
                )
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
        return message.content.map { appContext.getString(RCommon.string.products_message_custom_preview) }
    }

    @Composable
    override fun formatChatPreview(
        message: LastMessageUiModel.Custom<ProductsMessageContent>,
    ): Result<String> {
        return message.content.map { stringResource(RCommon.string.products_message_custom_preview) }
    }
}
