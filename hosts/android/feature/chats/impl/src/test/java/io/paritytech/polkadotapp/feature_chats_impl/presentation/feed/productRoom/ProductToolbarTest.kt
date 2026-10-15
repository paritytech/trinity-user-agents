package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductToolbarTest {
    @Test
    fun `two buttons go in the bar and the others in the overflow menu, in the order of the product`() {
        val actions = listOf("a", "b", "c", "d").map(::action)

        val toolbar = actions.toToolbar(icons = mapOf("b" to "icon-b", "d" to "icon-d"))

        assertEquals(listOf(item("a", null), item("b", "icon-b")), toolbar.inline)
        assertEquals(listOf(item("c", null), item("d", "icon-d")), toolbar.overflow)
    }

    @Test
    fun `two buttons or fewer need no overflow menu`() {
        val toolbar = listOf(action("a")).toToolbar(icons = emptyMap())

        assertEquals(listOf(item("a", null)), toolbar.inline)
        assertEquals(emptyList<ProductToolbarItemUiModel>(), toolbar.overflow)
    }

    private fun action(id: String) = ChatToolbarActionModel(
        actionId = id,
        label = id.uppercase(),
        icon = ChatResourceModel.Inline("image/png", byteArrayOf(1).toDataByteArray()),
        handler = ChatActionHandlerModel.Callback,
    )

    private fun item(id: String, icon: Any?) = ProductToolbarItemUiModel(id, id.uppercase(), icon)
}
