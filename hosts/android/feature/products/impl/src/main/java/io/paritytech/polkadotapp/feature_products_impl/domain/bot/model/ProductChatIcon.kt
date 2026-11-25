package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import uniffi.truapi.NativeChatFieldException
import uniffi.truapi.screenChatIcon
import uniffi.truapi.screenChatRoomIcon

/**
 * A room icon a product registered, screened by the core's chat icon rules. [fromString] and [fromCallback] are the
 * only ways to make one, so nothing that holds one needs to check it again.
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

        /**
         * As [fromString], for the icon that a chat callback of the core carries. That icon can also be the CID of a
         * Bulletin preimage. The room avatar does not resolve a preimage yet, so such a room shows its placeholder.
         */
        fun fromCallback(icon: String): Result<ProductChatIcon?> {
            val screened = try {
                screenChatRoomIcon(icon)
            } catch (refused: NativeChatFieldException.Refused) {
                return Result.failure(IllegalArgumentException(refused.reason))
            }

            return Result.success(screened.takeIf(String::isNotEmpty)?.let(::ProductChatIcon))
        }
    }
}
