package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.content.Context
import android.net.Uri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.asWebUri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.isPocketTarget
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketPublishError
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplink
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplinkParser
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * `polkadotapp://<product>.<tld>/-/pocket/{add,open}?card=<id>`. The first segment `-` is reserved
 * for host-handled targets, so the product App handler leaves these alone.
 */
internal class PocketDeepLinkHandler @Inject constructor(
    private val dispatchers: CoroutineDispatchers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val parser: PocketDeeplinkParser,
    private val publishedCards: PublishedPocketCards,
    private val collection: PocketCollection,
    private val gate: ProductDeepLinkGate,
    private val router: ProductsRouter,
    @param:ApplicationContext private val context: Context,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false
        if (!DotNsUtils.isDotDomain(data.asWebUri(), tld) || !data.isPocketTarget()) return false

        // Adding a card runs the product's worker and hosts its pages, so the same products are
        // reachable here as through any other dotNS link.
        return gate.opens(ProductId.fromUrl(data.asWebUri(), tld).getOrNull())
    }

    /**
     * The reserved target belongs to the host whether or not the link under it is well formed, so a
     * malformed one is answered here rather than left to open the product's own page.
     */
    private fun malformed(data: Uri): DeeplinkProcessingOutcome {
        Timber.w("pocket: %s is under the reserved target but is not a Pocket link", data)

        return DeeplinkProcessingOutcome.ShowMessage(context.getString(RCommon.string.pocket_deeplink_malformed))
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(dispatchers.io) {
        dotNsTldProvider.getTld().flatMap { tld ->
            val normalized = DotNsUtils.normalize(data.asWebUri(), tld)
                ?: return@flatMap Result.failure(IllegalArgumentException("Not a $tld domain: $data"))
            val deeplink = parser.parse(normalized.toString()) ?: return@flatMap Result.success(malformed(data))

            ProductId.fromString(deeplink.productHost, tld).flatMap { productId -> dispatch(productId, deeplink) }
        }
    }

    private suspend fun dispatch(productId: ProductId, deeplink: PocketDeeplink): Result<DeeplinkProcessingOutcome> {
        val key = PocketCardKey(productId, PocketCardId(deeplink.cardId))
        val present = collection.observeCards().first().any { it.key == key }

        // A card already held is opened, whichever action asked for it. One that is not held has to
        // be approved before it can be opened, so both actions lead to the same offer; what the
        // product publishes decides whether there is one to make.
        return if (present) {
            Result.success(DeeplinkProcessingOutcome.Navigate { router.openPocketCard(key) })
        } else {
            offerToAdd(key)
        }
    }

    private suspend fun offerToAdd(key: PocketCardKey): Result<DeeplinkProcessingOutcome> {
        val offer: Result<DeeplinkProcessingOutcome> = publishedCards.find(key.productId, key.cardId).map {
            DeeplinkProcessingOutcome.Navigate {
                router.openPocketAddCard(PocketAddCardPayload(key.productId.value, key.cardId.value))
            }
        }
        return offer.recoverCatching { failure ->
            if (failure is PocketPublishError) hostError(failure) else throw failure
        }
    }

    private fun hostError(error: PocketPublishError) = DeeplinkProcessingOutcome.ShowMessage(
        context.getString(
            when (error) {
                PocketPublishError.NoPocket -> RCommon.string.pocket_deeplink_no_pocket
                PocketPublishError.UnknownCard -> RCommon.string.pocket_deeplink_unknown_card
            },
        ),
    )
}
