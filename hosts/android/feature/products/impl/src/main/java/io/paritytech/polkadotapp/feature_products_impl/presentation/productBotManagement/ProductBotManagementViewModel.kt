package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.common.utils.requireSuffix
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCard
import io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement.ProductBotManagementInteractor
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class ProductBotManagementViewModel @Inject constructor(
    private val interactor: ProductBotManagementInteractor,
    private val router: ProductsRouter,
) : BaseViewModel(), ProductBotManagementContract {
    private val tld = interactor.currentTld()

    override val state = MutableStateFlow(ProductBotManagementState(tldSuffix = tld?.suffix.orEmpty()))

    init {
        interactor.observeProducts()
            .mapList { product ->
                ProductUiModel(
                    id = product.id,
                    name = product.name,
                    appUrl = product.id.toUrl(),
                )
            }
            .onEach { products -> state.update { it.copy(products = products) } }
            .launchIn(this)
    }

    override fun onBackClick() {
        router.back()
    }

    override fun onAddProductClick() {
        state.update { it.copy(dialogState = ProductDialogState.Form()) }
    }

    override fun onProductClick(productId: ProductId) {
        launch {
            router.openSpaBrowser(SpaBrowserPayload.ByProductId(productId.value))
        }
    }

    override fun onEditProductClick(productId: String) {
        val product = state.value.products.find { it.id.value == productId } ?: return
        launch {
            val workerUrl = interactor.getUserWorkerUrl(product.id).orEmpty()
            val card = interactor.getDebugCard(product.id)
            val appUrl = interactor.getAppUrl(product.id).orEmpty()
            state.update {
                it.copy(
                    dialogState = ProductDialogState.Form(
                        productId = product.id.value,
                        dotNsName = product.name,
                        workerUrl = workerUrl,
                        cardId = card?.cardId?.value.orEmpty(),
                        cardTitle = card?.title.orEmpty(),
                        previewUrl = card?.previewUrl.orEmpty(),
                        appUrl = appUrl,
                        openWithFaceAway = card?.faceShown == false,
                    )
                )
            }
        }
    }

    override fun onDeleteProductClick(productId: String) = launchUnit {
        interactor.deleteProduct(ProductId.fromStoredValue(productId))
            .onFailure(::showError)
    }

    override fun onDialogDismiss() {
        state.update { it.copy(dialogState = ProductDialogState.None) }
    }

    override fun onDotNsDomainChanged(dotNsDomain: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(dotNsName = dotNsDomain))
        }
    }

    override fun onWorkerUrlChanged(url: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(workerUrl = url))
        }
    }

    override fun onCardIdChanged(cardId: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(cardId = cardId))
        }
    }

    override fun onCardTitleChanged(title: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(cardTitle = title))
        }
    }

    override fun onPreviewUrlChanged(url: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(previewUrl = url))
        }
    }

    override fun onAppUrlChanged(url: String) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(appUrl = url))
        }
    }

    override fun onOpenWithFaceAwayChanged(faceAway: Boolean) {
        state.update {
            val form = it.dialogState as? ProductDialogState.Form ?: return@update it
            it.copy(dialogState = form.copy(openWithFaceAway = faceAway))
        }
    }

    override fun onDialogConfirm() = launchUnit {
        val form = state.value.dialogState as? ProductDialogState.Form ?: return@launchUnit
        if (form.workerUrl.isBlank() || form.dotNsName.isBlank()) return@launchUnit

        state.update {
            it.copy(dialogState = form.copy(isSubmitting = true))
        }

        val card = form.toDebugCard()
        val appUrl = form.appUrl.trim().ifEmpty { null }

        val result = if (form.productId != null) {
            interactor.updateProduct(ProductId.fromStoredValue(form.productId), form.workerUrl, form.dotNsName, card, appUrl)
        } else {
            runCatching { requireNotNull(interactor.currentTld()) { "Network TLD is not known yet" } }
                .flatMap { tld ->
                    // Typed by hand in the debug menu, so it is validated rather than trusted.
                    val dotNs = form.dotNsName.lowercase().requireSuffix(tld.suffix)

                    ProductId.fromString(dotNs, tld)
                }
                .flatMap { productId ->
                    interactor.upsertProduct(productId, form.workerUrl, form.dotNsName, card, appUrl)
                }
        }

        result
            .onSuccess {
                state.update { it.copy(dialogState = ProductDialogState.None) }
            }
            .onFailure { error ->
                showError(error)

                state.update {
                    it.copy(dialogState = form.copy(isSubmitting = false))
                }
            }
    }

    /**
     * A card needs both an id and a face to be worth declaring; either one alone would produce a
     * worker whose card cannot be drawn. Screening the id is left to the store, which applies the
     * same rules the manifest path does.
     */
    private fun ProductDialogState.Form.toDebugCard(): DebugPocketCard? {
        if (cardId.isBlank() || previewUrl.isBlank()) return null

        return DebugPocketCard(
            cardId = PocketCardId(cardId.trim()),
            title = cardTitle.ifBlank { cardId }.trim(),
            previewUrl = previewUrl.trim(),
            faceShown = !openWithFaceAway,
        )
    }
}
