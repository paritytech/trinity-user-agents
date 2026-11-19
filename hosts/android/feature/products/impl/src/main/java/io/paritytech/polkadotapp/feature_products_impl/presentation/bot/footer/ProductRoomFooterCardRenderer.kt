package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatInputSlotRenderer
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.common.R as RCommon

/** Draws the card a product docked in the input slot of [room], over a fade from the feed. */
class ProductRoomFooterCardRenderer(
    private val room: ProductChatIdParameter,
    val card: ProductRoomFooter.Card,
    private val worker: ProductWorker,
    private val imageResolver: JsImageResolver,
) : CustomChatInputSlotRenderer {
    @Composable
    override fun drawInputSlot() {
        val viewModel: ProductRoomFooterCardViewModel = hiltViewModel(
            key = "room-footer-${room.value}",
            creationCallback = { factory: ProductRoomFooterCardViewModel.Factory ->
                factory.create(room.value, worker)
            }
        )
        LaunchedEffect(card) { viewModel.show(card) }
        val state by viewModel.state.collectAsState()

        val background = PolkadotTheme.colors.bg.surface.main
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .background(Brush.verticalGradient(listOf(Color.Transparent, background, background)))
                .navigationBarsPadding()
                .imePadding()
                .padding(horizontal = 16.dp, vertical = 12.dp)
        ) {
            when (val currentState = state) {
                is LoadingState.Loading -> {
                    CircularProgressIndicator(
                        modifier = Modifier
                            .size(24.dp)
                            .align(Alignment.Center),
                        color = PolkadotTheme.colors.fg.secondary,
                        strokeWidth = 2.dp,
                    )
                }
                is LoadingState.Loaded -> {
                    CompositionLocalProvider(LocalJsImageResolver provides imageResolver) {
                        JsWidgetRenderer(
                            widget = currentState.data,
                            jsEventHandler = viewModel::handleUiEvent,
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
    }

    override fun equals(other: Any?): Boolean = other is ProductRoomFooterCardRenderer && room == other.room && card == other.card

    override fun hashCode(): Int = 31 * room.hashCode() + card.hashCode()
}
