package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import uniffi.truapi.NavigateDecision
import uniffi.truapi.parseNavigate
import javax.inject.Inject
import uniffi.truapi.PocketDeeplinkAction as NativePocketDeeplinkAction

enum class PocketDeeplinkAction { ADD, OPEN }

/**
 * A `/-/pocket/<action>?card=<id>` ([Card]) or bare `/-/pocket` ([Collection]) deeplink as the core classifies it;
 * [productHost] is the lower-cased dotNS name.
 */
sealed interface PocketDeeplink {
    val productHost: String

    data class Card(
        override val productHost: String,
        val action: PocketDeeplinkAction,
        val cardId: String,
    ) : PocketDeeplink

    data class Collection(override val productHost: String) : PocketDeeplink
}

/**
 * Classifies through the core's `parse_navigate`, so every host reads a Pocket deeplink the same
 * way. Kept behind this adapter, like the host bridges, so a bindgen rename does not ripple.
 */
class PocketDeeplinkParser @Inject constructor() {
    fun parse(url: String): PocketDeeplink? = when (val decision = parseNavigate(url)) {
        is NavigateDecision.Pocket -> PocketDeeplink.Card(
            productHost = decision.identifier,
            action = when (decision.action) {
                NativePocketDeeplinkAction.ADD -> PocketDeeplinkAction.ADD
                NativePocketDeeplinkAction.OPEN -> PocketDeeplinkAction.OPEN
            },
            cardId = decision.cardId,
        )

        is NavigateDecision.PocketCollection -> PocketDeeplink.Collection(productHost = decision.identifier)

        else -> null
    }
}
