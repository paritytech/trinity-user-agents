package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

/** What the room screen shows of the parts of a product room that its product declares. */
@Immutable
data class ProductRoomUiState(
    val toolbar: ProductToolbarUiModel,
) {
    companion object {
        val None = ProductRoomUiState(
            toolbar = ProductToolbarUiModel(inline = persistentListOf(), overflow = persistentListOf()),
        )
    }
}

/** A button of the toolbar of a product room. [icon] is what the image loader draws, or null until it loads. */
@Immutable
data class ProductToolbarItemUiModel(val actionId: String, val label: String, val icon: Any?)

/** The buttons that the bar shows, and the buttons of the overflow menu, in the order of the product. */
@Immutable
data class ProductToolbarUiModel(
    val inline: ImmutableList<ProductToolbarItemUiModel>,
    val overflow: ImmutableList<ProductToolbarItemUiModel>,
) {
    companion object {
        /** The number of buttons that the bar shows (02-DESIGN §6.6). */
        const val INLINE_COUNT = 2
    }
}
