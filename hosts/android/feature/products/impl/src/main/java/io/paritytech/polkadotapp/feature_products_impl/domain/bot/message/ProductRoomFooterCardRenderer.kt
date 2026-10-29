package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker

/** Draws the card a product docked in the input slot of [room], over a fade from the feed. */
class ProductRoomFooterCardRenderer(
    private val room: ProductChatIdParameter,
    val card: ProductRoomFooter.Card,
    private val worker: ProductWorker,
    private val imageResolver: JsImageResolver,
) : CustomChatFooterRenderer {
    @Composable
    override fun drawFooter() {
        var widget by remember(room) { mutableStateOf<JsWidget?>(null) }
        LaunchedEffect(card) {
            worker.renderRoomFooter(room, card.messageType, card.payload.toDataByteArray())
                .collect { result ->
                    result.logFailure("Room footer card render failed in ${room.value}")
                        .onSuccess { widget = it }
                }
        }
        val background = PolkadotTheme.colors.bg.surface.main
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .background(Brush.verticalGradient(listOf(Color.Transparent, background, background)))
                .navigationBarsPadding()
                .imePadding()
                .padding(horizontal = 16.dp, vertical = 12.dp)
        ) {
            widget?.let {
                CompositionLocalProvider(LocalJsImageResolver provides imageResolver) {
                    JsWidgetRenderer(
                        widget = it,
                        jsEventHandler = { actionId, eventType ->
                            worker.dispatchRoomFooterEvent(room, card.messageType, actionId, eventType)
                        },
                    )
                }
            }
        }
    }

    override fun equals(other: Any?): Boolean = other is ProductRoomFooterCardRenderer && room == other.room && card == other.card

    override fun hashCode(): Int = 31 * room.hashCode() + card.hashCode()
}
