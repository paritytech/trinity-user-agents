package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.content.Context
import android.net.Uri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardOpenRequests
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
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
 * `polkadotapp://pocket/{add,open}?product=<id>&card=<id>`, and `polkadotapp://pocket` alone for the collection, from a
 * product's `navigateTo` or from outside the app alike.
 */
internal class PocketDeepLinkHandler @Inject constructor(
    private val dispatchers: CoroutineDispatchers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val parser: PocketDeeplinkParser,
    private val publishedCards: PublishedPocketCards,
    private val collection: PocketCollection,
    private val cardOpenRequests: PocketCardOpenRequests,
    private val gate: ProductDeepLinkGate,
    private val accountRepository: AccountRepository,
    private val router: ProductsRouter,
    @param:ApplicationContext private val context: Context,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        if (!isPocketLink(data.scheme, data.host)) return false
        val card = parser.parse(data.toString()) as? PocketDeeplink.Card ?: return true
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false

        // Adding a card runs the product's worker and hosts its pages, so the same products are
        // reachable here as through any other dotNS link.
        return gate.opens()
    }

    /**
     * A link to the Pocket belongs to the host whether or not it is well formed, so a malformed one
     * is answered here rather than dropped.
     */
    private fun malformed(data: Uri): DeeplinkProcessingOutcome {
        Timber.w("pocket: %s names the Pocket but is not a Pocket link the core reads", data)

        return DeeplinkProcessingOutcome.ShowMessage(context.getString(RCommon.string.pocket_deeplink_malformed))
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(dispatchers.io) {
        when (val deeplink = parser.parse(data.toString())) {
            null -> Result.success(malformed(data))
            PocketDeeplink.Collection -> openCollection()
            is PocketDeeplink.Card -> dotNsTldProvider.getTld()
                .flatMap { tld -> ProductId.fromString(deeplink.productHost, tld) }
                .flatMap { productId -> dispatchCard(PocketCardKey(productId, PocketCardId(deeplink.cardId))) }
        }
    }

    private suspend fun openCollection(): Result<DeeplinkProcessingOutcome> = runCancellableCatching {
        accountRepository.awaitAccountsInitialized()

        DeeplinkProcessingOutcome.Navigate { router.openWalletTab() }
    }

    private suspend fun dispatchCard(key: PocketCardKey): Result<DeeplinkProcessingOutcome> {
        val present = collection.observeCards().first().any { it.key == key }

        // A card already held is opened, whichever action asked for it. One that is not held has to
        // be approved before it can be opened, so both actions lead to the same offer; what the
        // product publishes decides whether there is one to make.
        return if (present) {
            Result.success(
                DeeplinkProcessingOutcome.Navigate {
                    cardOpenRequests.request(key)
                    router.openWalletTab()
                },
            )
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
