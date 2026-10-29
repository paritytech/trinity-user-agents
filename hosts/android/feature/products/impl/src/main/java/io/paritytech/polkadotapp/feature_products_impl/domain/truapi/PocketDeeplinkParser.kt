package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import uniffi.truapi.NavigateDecision
import uniffi.truapi.parseNavigate
import javax.inject.Inject
import uniffi.truapi.PocketDeeplinkAction as NativePocketDeeplinkAction

enum class PocketDeeplinkAction { ADD, OPEN }

/**
 * A `polkadotapp://pocket/<action>?product=<id>&card=<id>` ([Card]) or bare `polkadotapp://pocket` ([Collection])
 * deeplink as the core classifies it; [Card.productHost] is the product's lower-cased dotNS name.
 */
sealed interface PocketDeeplink {
    data class Card(
        val productHost: String,
        val action: PocketDeeplinkAction,
        val cardId: String,
        val pageQuery: String,
    ) : PocketDeeplink

    data object Collection : PocketDeeplink
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
            pageQuery = decision.pageQuery,
        )

        is NavigateDecision.PocketCollection -> PocketDeeplink.Collection

        else -> null
    }
}
