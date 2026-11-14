package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatInputSlotRenderer
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import uniffi.truapi.ProductId

/** Shows the page a product docked in the input slot of [room], from the product's own web content. */
class ProductRoomFooterWebRenderer(
    private val room: ProductChatIdParameter,
    private val productId: ProductId,
    val footer: ProductRoomFooter.Web,
    private val worker: ProductWorker,
) : CustomChatInputSlotRenderer {
    private val pageUrl: String? = ProductRoomFooterWebPage.url(footer.path, productId.value)

    @Composable
    override fun drawInputSlot() {
        if (pageUrl != null) {
            WebFooter(pageUrl)
        }
    }

    @Composable
    private fun WebFooter(url: String) {
        val viewModel: ProductRoomFooterWebViewModel = hiltViewModel(
            key = "room-footer-web-${room.value}",
            creationCallback = { factory: ProductRoomFooterWebViewModel.Factory ->
                factory.create(room.value, worker)
            }
        )
        var heightDp by remember(footer) { mutableIntStateOf(ProductRoomFooterWebPage.height(footer.height)) }
        var layout by remember { mutableStateOf<ProductRoomFooterWebLayout?>(null) }

        LaunchedEffect(viewModel) {
            viewModel.hostEvents.collect { event -> layout?.send(event) }
        }

        Box(
            modifier = Modifier
                .fillMaxWidth()
                // The slot is the composer area: the chat's own background, so messages scrolled under it stay hidden.
                // The webview is transparent and only its overlay, drawn above this box, lets the chat show through.
                .background(PolkadotTheme.colors.bg.surface.main)
                .navigationBarsPadding()
                .imePadding()
        ) {
            // A new page is a new webview; the old one goes with its page.
            key(url) {
                AndroidView(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(heightDp.dp),
                    factory = { context ->
                        ProductRoomFooterWebLayout(
                            context = context,
                            productId = productId.value,
                            pageUrl = url,
                            content = viewModel.content,
                            onAction = viewModel::onPageAction,
                            onHeight = { heightDp = it },
                        ).also { layout = it }
                    },
                    onRelease = { released ->
                        released.release()
                        if (layout === released) layout = null
                    },
                )
            }
        }
    }

    override fun equals(other: Any?): Boolean =
        other is ProductRoomFooterWebRenderer && room == other.room && productId == other.productId && footer == other.footer

    override fun hashCode(): Int = 31 * (31 * room.hashCode() + productId.hashCode()) + footer.hashCode()
}
