package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.presentation.ProductFooterViewModel

/** The host bounds the footer that a product draws to this part of the screen height (02-DESIGN §6.6). */
private const val FOOTER_MAX_HEIGHT_FRACTION = 0.4f

/** Draws the `Custom` footer of a product room in the footer slot of the room screen (P6.6). */
class ProductFooterRenderer(
    private val room: ProductChatIdParameter,
    private val worker: ProductWorker,
) : CustomChatFooterRenderer {
    @Composable
    override fun drawFooter() {
        val viewModel: ProductFooterViewModel = hiltViewModel(
            key = "footer:${room.value}",
            creationCallback = { factory: ProductFooterViewModel.Factory -> factory.create(worker, room.value) },
        )
        val state by viewModel.state.collectAsState()
        val maxHeight = LocalConfiguration.current.screenHeightDp.dp * FOOTER_MAX_HEIGHT_FRACTION

        (state as? LoadingState.Loaded)?.let { loaded ->
            JsWidgetRenderer(
                widget = loaded.data,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = maxHeight)
                    .verticalScroll(rememberScrollState()),
                jsEventHandler = viewModel::handleUiEvent,
            )
        }
    }
}
