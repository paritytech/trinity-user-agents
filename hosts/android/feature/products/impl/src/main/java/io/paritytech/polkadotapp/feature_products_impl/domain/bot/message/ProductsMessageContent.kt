package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.Serializable

/**
 * Content for custom messages rendered by Products bots/scripts.
 *
 * The widget tree is NOT stored here - it's generated at render time by invoking
 * the script that is related to the bot.
 *
 * @param messageType Type identifier for the custom message (used by the rendering pipeline)
 * @param data Opaque byte array - the script is responsible for encoding/decoding this data
 * @param alt The product's one-line description, shown where the message is listed rather than drawn
 */
@Serializable
data class ProductsMessageContent(
    val messageType: String,
    val data: DataByteArray,
    val alt: String? = null,
)
