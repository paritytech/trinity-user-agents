package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import javax.inject.Inject

class ProductPersonChatRooms @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
) : PersonChatRooms {
    override fun homeRoom(): ChatId? {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return null
        return ProductChatIdParameter(HOME_ROOM).toChatId(ReservedProductIds.chat(tld).toChatExtensionId())
    }

    override fun isPersonChatRoom(chatId: ChatId): Boolean {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false
        return chatId.extensionOrNull()?.extensionId == ReservedProductIds.chat(tld).toChatExtensionId()
    }

    private companion object {
        // The room id that the chat product takes `open_link` on (`packages/product/src/surface/mapping.ts`).
        const val HOME_ROOM = "chats"
    }
}
