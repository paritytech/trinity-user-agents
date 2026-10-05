package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import android.content.Context
import android.net.Uri
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.ContentSharing
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.PocketInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketScreenState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.job
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class PocketViewModel @Inject constructor(
    private val interactor: PocketInteractor,
    private val tokenAmountMapper: TokenAmountMapper,
    private val tokenAmountFormatter: TokenAmountFormatter,
    private val router: PocketRouter,
    private val collectiblesUrlResolver: CollectiblesUrlResolver,
    private val idShareImageRenderer: IdShareImageRenderer,
    private val sharingManager: SharingManager,
    private val dispatchers: CoroutineDispatchers,
    spaHost: SpaHost,
    @param:ApplicationContext private val context: Context
) : BaseViewModel() {
    private val selectedCardId = MutableStateFlow<String?>(null)
    private val expandedProduct = ExpandedProductPage(this) { scope, url -> with(scope) { spaHost.createSession(url) } }
    private val collectiblesShown = MutableStateFlow(false)
    private val removalCandidate = MutableStateFlow<PocketCardUiModel.ProductCard?>(null)

    private val digitalDollarAmounts = interactor.observeDigitalDollarBalance()
        .map { balance ->
            PocketCardUiModel.DigitalDollar.Amounts(
                balance = tokenAmountMapper.mapFrom(balance.total),
                ready = tokenAmountMapper.mapFrom(balance.ready)
            )
        }
        .withLoading("PocketViewModel: Failed to observe digital dollar balance")

    // Both upstreams start unresolved so the card is on screen from the first frame, shimmering its
    // amount, instead of appearing only once a balance arrives.
    private val balanceCard = combine(
        digitalDollarAmounts,
        interactor.observeBackupProgress().onStart { emit(BackupProgress.Unknown) },
        interactor.observeAccountBackupPending().onStart { emit(false) },
    ) { amounts, backupProgress, accountBackupPending ->
        PocketCardUiModel.DigitalDollar(
            amounts = amounts,
            syncInProgress = backupProgress.isInProgress(),
            accountBackupPending = accountBackupPending,
        )
    }

    private val addressCard = combine<_, _, _, PocketCardUiModel.IdCard?>(
        interactor.observeUsername(),
        interactor.observeRank(),
        interactor.observeAddress()
    ) { username, rank, address ->
        PocketCardUiModel.IdCard(username = username, address = address, rank = rank)
    }.onStart { emit(null) }

    private val warmedPrivilegedProducts = ConcurrentHashMap.newKeySet<ProductId>()

    // Native cards keep their place; the product-backed collection follows, pinned cards first.
    // A collection the host cannot read costs the user their product cards, never the balance and
    // identity cards standing beside them.
    private val productCards = interactor.observeProductCards()
        .map { cards -> cards.map { it.toUiModel() } }
        .onEach(::warmUpPrivilegedProducts)
        .catch { failure ->
            Timber.e(failure, "PocketViewModel: the product card collection is unavailable")
            emit(emptyList())
        }
        .onStart { emit(emptyList()) }

    val cards = combine(balanceCard, addressCard, productCards) { balance, address, products ->
        (listOfNotNull(balance, address) + products).toImmutableList()
    }
        .distinctUntilChangedBy { cards -> cards.map(::cardDisplayKey) }
        .onEach(::forgetCardsNoLongerHeld)
        .flowOn(dispatchers.computation)
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = persistentListOf()
        )

    val collectiblesAvailable = flowOf {
        collectiblesUrlResolver.resolveUrl() != null
    }
        .flowOn(dispatchers.computation)
        .stateIn(scope = this, started = SharingStarted.Eagerly, initialValue = false)

    val state: StateFlow<PocketScreenState> = combine(
        cards,
        selectedCardId,
        collectiblesShown,
        collectiblesAvailable,
        removalCandidate
    ) { cards, selectedId, collectiblesShown, collectiblesAvailable, candidate ->
        val selectedCard = cards.firstOrNull { it.id == selectedId }
        when {
            selectedCard != null -> PocketScreenState.CardDetails(selectedCard = selectedCard)
            collectiblesShown -> PocketScreenState.Collectibles
            else -> PocketScreenState.List(
                collectiblesAvailable = collectiblesAvailable,
                removalCandidate = candidate
            )
        }
    }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = PocketScreenState.List(collectiblesAvailable = false, removalCandidate = null)
        )

    private class CardBinding(val scope: CoroutineScope, val bindings: ProductFaceBindings)

    private val bindings = ConcurrentHashMap<String, CardBinding>()

    /**
     * One set of bindings per card, however many copies of it are drawn: expanding a card draws a
     * second one over the first, and two streams would take two worker references and leave the new
     * copy empty until the product drew again. The newest face is replayed to whichever copy asks next.
     *
     * The stream outlives a short gap in subscribers, so scrolling the card off screen or leaving the
     * app for a moment does not tear the product's worker down and boot it again on the way back.
     */
    fun bindingsOf(card: PocketCardUiModel.ProductCard): ProductFaceBindings =
        bindings.getOrPut(card.id) { bindingFor(card) }.bindings

    private fun bindingFor(card: PocketCardUiModel.ProductCard): CardBinding {
        // Its own child of the screen: sharing runs until its scope ends, whatever the subscriber
        // count, so a card that leaves the collection would otherwise leave a coroutine parked here
        // with a whole face tree in its replay buffer.
        val scope = CoroutineScope(coroutineContext + SupervisorJob(coroutineContext.job))

        return CardBinding(
            scope = scope,
            bindings = ProductFaceBindings(
                face = interactor.observeFace(card.key)
                    .stateIn(scope, SharingStarted.WhileSubscribed(WORKER_KEEP_ALIVE_MILLIS), initialValue = null),
                onFaceAction = { actionId, type -> onFaceAction(card, actionId, type) },
                imageResolver = object : JsImageResolver {
                    override suspend fun resolve(source: JsImageSource) = resolveFaceImage(card, source)

                    override fun resolved(source: JsImageSource) = resolvedFaceImages[card.id to source]
                },
            ),
        )
    }

    private fun cardDisplayKey(card: PocketCardUiModel): String = when (card) {
        is PocketCardUiModel.DigitalDollar -> {
            val amounts = card.amounts.dataOrNull

            listOf(
                amounts?.let {
                    tokenAmountFormatter.formatTokenAmount(it.balance, RoundPrecision.FIAT, withSymbol = false)
                },
                amounts?.let { tokenAmountFormatter.formatTokenAmount(it.ready, RoundPrecision.FIAT, withSymbol = false) },
                card.syncInProgress,
                card.accountBackupPending,
                amounts?.notFullyReady
            ).joinToString("|")
        }

        is PocketCardUiModel.IdCard -> listOf(card.username, card.address, card.rank).joinToString("|")

        is PocketCardUiModel.ProductCard -> listOf(card.id, card.title, card.pinned).joinToString("|")
    }

    private fun PocketCard.toUiModel() = PocketCardUiModel.ProductCard(
        key = key,
        title = title,
        pinned = privileged
    )

    /** The product page under the expanded card, live only while that card is open. */
    val expandedProductSession = expandedProduct.session

    fun selectCard(card: PocketCardUiModel) {
        selectedCardId.value = card.id
        if (card is PocketCardUiModel.ProductCard) warmUpProduct(card)
    }

    /**
     * The card travels for half a second before its product is asked for. Fetching the product's
     * pages now spends that half second on the chain read and the download rather than in front of
     * them, and the load that follows joins this fetch instead of starting a second one.
     */
    private fun warmUpProduct(card: PocketCardUiModel.ProductCard) = launchUnit {
        interactor.warmUpProduct(card.key).logFailure("PocketViewModel: failed to warm up ${card.id}")
    }

    /**
     * Only the host places a privileged card, so this set cannot grow with use. Fetching its pages
     * when the collection arrives spends the time the user spends looking at the cards, rather than
     * the half second [selectCard] has to offer.
     */
    private fun warmUpPrivilegedProducts(cards: List<PocketCardUiModel.ProductCard>) {
        cards.filter { it.pinned && warmedPrivilegedProducts.add(it.key.productId) }
            .forEach(::warmUpProduct)
    }

    fun dismissCard() {
        expandedProduct.close()
        selectedCardId.value = null
    }

    fun showCollectiblesSketchbook() {
        collectiblesShown.value = true
    }

    fun hideCollectiblesSketchbook() {
        collectiblesShown.value = false
    }

    fun openCollectibles() {
        router.openCollectibles()
    }

    /**
     * The product is loaded only once the card has finished travelling. Building a WebView while the
     * card is still moving starves the animation, and the card is the part the user is watching.
     */
    fun hostExpandedProduct(card: PocketCardUiModel.ProductCard) {
        expandedProduct.open(card.key.launchUrl())
    }

    /** A press or edit inside a face goes back to the product; a text edit carries the new value as UTF-8. */
    private fun onFaceAction(card: PocketCardUiModel.ProductCard, actionId: String, type: JsUiEvent.Type) {
        val payload = when (type) {
            JsUiEvent.Type.ButtonClick -> ByteArray(0)
            is JsUiEvent.Type.InputFieldValueChange -> type.newValue.toByteArray()
        }
        interactor.sendFaceAction(card.key, actionId, payload)
    }

    /**
     * Kept for as long as the screen lives. A card expanding draws a second copy of itself, which
     * would otherwise fetch the same image again and show nothing until it arrived.
     */
    private val resolvedFaceImages = ConcurrentHashMap<Pair<String, JsImageSource>, Uri>()

    private suspend fun resolveFaceImage(card: PocketCardUiModel.ProductCard, source: JsImageSource): Uri? =
        resolvedFaceImages[card.id to source]
            ?: interactor.resolveFaceImage(card.key, source)
                .logFailure("PocketViewModel: face image unavailable for ${card.id}")
                .getOrNull()
                ?.also { resolvedFaceImages[card.id to source] = it }

    fun requestRemoval(card: PocketCardUiModel.ProductCard) {
        if (!card.pinned) removalCandidate.value = card
    }

    fun dismissRemoval() {
        removalCandidate.value = null
    }

    fun confirmRemoval() = launchUnit {
        val candidate = removalCandidate.value ?: return@launchUnit
        removalCandidate.value = null

        interactor.removeProductCard(candidate.key).logFailure("PocketViewModel: failed to remove ${candidate.id}")
    }

    /**
     * A card can leave the collection while the user is looking at it: its product removes it, or
     * the collection itself becomes unreadable. The screen falls back to the list on its own, so
     * everything the card was holding open has to go with it.
     */
    private fun forgetCardsNoLongerHeld(held: List<PocketCardUiModel>) {
        val ids = held.map { it.id }.toSet()
        selectedCardId.value?.takeIf { it !in ids }?.let { closeExpandedCard() }
        expandedProduct.keepOnly { url -> held.holdsProductAt(url) }
        bindings.keys.filterNot { it in ids }.forEach { bindings.remove(it)?.scope?.cancel() }
        resolvedFaceImages.keys.removeAll { (cardId, _) -> cardId !in ids }
    }

    private fun List<PocketCardUiModel>.holdsProductAt(url: String): Boolean =
        filterIsInstance<PocketCardUiModel.ProductCard>().any { it.key.launchUrl() == url }

    private fun closeExpandedCard() {
        expandedProduct.release()
        selectedCardId.value = null
    }

    fun onShareId() = launchUnit {
        val idCard = cards.value.filterIsInstance<PocketCardUiModel.IdCard>().firstOrNull() ?: return@launchUnit

        interactor.getAppSharingUrl()
            .onSuccess { url -> shareId(idCard, url) }
            .onFailure { showPresentationError(ShareIdFailedPresentationError(it)) }
    }

    private suspend fun shareId(idCard: PocketCardUiModel.IdCard, appSharingUrl: String) {
        val text = context.getString(RCommon.string.pocket_id_share_message, appSharingUrl, idCard.username)

        idShareImageRenderer.render(idCard.address)
            .logFailure("PocketViewModel: failed to render ID share image")
            .onSuccess { uri ->
                sharingManager.shareContent(
                    ContentSharing.file(
                        text = text,
                        uri = uri,
                        mimeType = "image/jpeg"
                    )
                )
            }
            .onFailure { sharingManager.shareText(text) }
    }

    private companion object {
        // Long enough to cover a scroll away and back, or a glance at another app.
        const val WORKER_KEEP_ALIVE_MILLIS = 5_000L
    }
}
