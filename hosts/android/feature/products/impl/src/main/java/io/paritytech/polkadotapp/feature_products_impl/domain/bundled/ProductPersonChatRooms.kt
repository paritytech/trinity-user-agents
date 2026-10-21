package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
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

    private companion object {
        // The chat product's `HOME_ROOM` (`packages/product/src/surface/mapping.ts`).
        const val HOME_ROOM = "chats"
    }
}
