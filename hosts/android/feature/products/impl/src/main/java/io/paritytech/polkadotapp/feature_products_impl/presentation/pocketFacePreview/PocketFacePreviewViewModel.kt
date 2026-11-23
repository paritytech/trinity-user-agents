package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import android.content.Context
import coil.request.CachePolicy
import coil.request.ImageRequest
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview.PocketFacePreviewInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import javax.inject.Inject

@HiltViewModel
class PocketFacePreviewViewModel @Inject constructor(
    private val interactor: PocketFacePreviewInteractor,
    private val router: ProductsRouter,
    @param:ApplicationContext private val context: Context,
) : BaseViewModel(), PocketFacePreviewContract {
    /** The attempt counter is what makes drawing the same URL again actually re-fetch it. */
    private data class Request(val url: String, val backUrl: String?, val attempt: Int)

    private class Drawn(val draws: Int, val face: LoadingState<JsWidget>?, val back: LoadingState<JsWidget>?)

    private val typedUrl = MutableStateFlow("")
    private val typedBackUrl = MutableStateFlow("")
    private val requests = MutableStateFlow<Request?>(null)

    @OptIn(ExperimentalCoroutinesApi::class)
    private val drawn: Flow<Drawn> = requests.flatMapLatest { request ->
        if (request == null) {
            flowOf(Drawn(draws = 0, face = null, back = null))
        } else {
            combine(request.url.load(), request.backUrl?.load() ?: flowOf(null)) { face, back -> Drawn(request.attempt, face, back) }
        }
    }

    override val state: StateFlow<PocketFacePreviewState> =
        combine(typedUrl, typedBackUrl, drawn) { url, backUrl, drawn ->
            PocketFacePreviewState(url = url, backUrl = backUrl, face = drawn.face, back = drawn.back, draws = drawn.draws)
        }
            .stateIn(this, SharingStarted.Eagerly, PocketFacePreviewState(url = "", backUrl = "", face = null, back = null, draws = 0))

    override val imageResolver = imagesFrom { it.url }

    override val backImageResolver = imagesFrom { it.backUrl }

    private fun imagesFrom(faceUrlOf: (Request) -> String?) = JsImageResolver { source ->
        val faceUrl = requests.value?.let(faceUrlOf) ?: return@JsImageResolver null

        val address = interactor.resolveImage(faceUrl, source)
            .logFailure("PocketFacePreview: face image unavailable")
            .getOrNull()

        // Drawing again is how an edited archive image is picked up, so no cache may answer for the dev server.
        // A Bulletin CID names content that never changes, so it keeps the cache.
        if (address == null || source is JsImageSource.Bulletin) {
            address
        } else {
            ImageRequest.Builder(context)
                .data(address)
                .memoryCachePolicy(CachePolicy.DISABLED)
                .diskCachePolicy(CachePolicy.DISABLED)
                .build()
        }
    }

    override fun onBackClick() {
        router.back()
    }

    override fun onUrlChanged(url: String) {
        typedUrl.value = url
    }

    override fun onBackUrlChanged(url: String) {
        typedBackUrl.value = url
    }

    override fun onDrawClick() {
        val url = typedUrl.value.trim()
        if (url.isBlank()) return

        val backUrl = typedBackUrl.value.trim().ifBlank { null }
        requests.update { Request(url = url, backUrl = backUrl, attempt = (it?.attempt ?: 0) + 1) }
    }

    private fun String.load(): Flow<LoadingState<JsWidget>> = flow {
        emit(LoadingState.Loading)

        val drawn = interactor.loadFace(this@load)
            .fold(onSuccess = { LoadingState.Loaded(it) }, onFailure = { LoadingState.Error(it) })

        emit(drawn)
    }
}
