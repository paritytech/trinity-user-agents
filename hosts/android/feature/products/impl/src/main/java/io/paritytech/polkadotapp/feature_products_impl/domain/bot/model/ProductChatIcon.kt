package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import uniffi.truapi.NativeChatFieldException
import uniffi.truapi.screenChatIcon

/**
 * A room icon a product registered, screened by the core's chat icon rules. [fromString] is the only way to
 * make one, so nothing that holds one needs to check it again.
 */
@JvmInline
value class ProductChatIcon private constructor(val value: String) {
    companion object {
        /** Null when the product gave no icon, and a failure when the core refuses the one it gave. */
        fun fromString(icon: String?): Result<ProductChatIcon?> {
            val screened = try {
                screenChatIcon(icon.orEmpty())
            } catch (refused: NativeChatFieldException.Refused) {
                return Result.failure(IllegalArgumentException(refused.reason))
            }

            return Result.success(screened.takeIf(String::isNotEmpty)?.let(::ProductChatIcon))
        }
    }
}
