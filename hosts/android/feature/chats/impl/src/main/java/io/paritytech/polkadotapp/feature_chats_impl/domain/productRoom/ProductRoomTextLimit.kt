package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat

/** The most bytes of the text of a message in a product room (`CHAT_BODY_MAX_BYTES`, 02-DESIGN §3.5). */
const val PRODUCT_ROOM_TEXT_MAX_BYTES = 16 * 1024

/**
 * False for a text that is too long for the room. The core refuses a longer text from the product, so the product
 * cannot give it back. The room screen keeps such a text in the input and tells the user.
 */
fun ChatId.takesText(text: String): Boolean = !isExtensionChat() || text.encodeToByteArray().size <= PRODUCT_ROOM_TEXT_MAX_BYTES
