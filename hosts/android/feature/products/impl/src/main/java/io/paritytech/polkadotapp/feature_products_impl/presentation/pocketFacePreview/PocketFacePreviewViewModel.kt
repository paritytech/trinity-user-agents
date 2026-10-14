package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
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
) : BaseViewModel(), PocketFacePreviewContract {
    /** The attempt counter is what makes drawing the same URL again actually re-fetch it. */
    private data class Request(val url: String, val attempt: Int)

    private val typedUrl = MutableStateFlow("")
    private val requests = MutableStateFlow<Request?>(null)

    @OptIn(ExperimentalCoroutinesApi::class)
    private val face: Flow<LoadingState<JsWidget>?> = requests.flatMapLatest { request ->
        if (request == null) flowOf(null) else request.url.load()
    }

    override val state: StateFlow<PocketFacePreviewState> =
        combine(typedUrl, face) { url, face -> PocketFacePreviewState(url = url, face = face) }
            .stateIn(this, SharingStarted.Eagerly, PocketFacePreviewState(url = "", face = null))

    override fun onBackClick() {
        router.back()
    }

    override fun onUrlChanged(url: String) {
        typedUrl.value = url
    }

    override fun onDrawClick() {
        val url = typedUrl.value.trim()
        if (url.isBlank()) return

        requests.update { Request(url = url, attempt = (it?.attempt ?: 0) + 1) }
    }

    private fun String.load(): Flow<LoadingState<JsWidget>> = flow {
        emit(LoadingState.Loading)

        val drawn = interactor.loadFace(this@load)
            .fold(onSuccess = { LoadingState.Loaded(it) }, onFailure = { LoadingState.Error(it) })

        emit(drawn)
    }
}
