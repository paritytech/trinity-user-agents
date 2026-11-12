package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIContactPicks
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject

@HiltViewModel
class TrUAPIContactPickViewModel @Inject constructor(
    private val router: ProductsRouter,
    picks: TrUAPIContactPicks,
) : BaseViewModel(), TrUAPIContactPickContract {
    private val prompt = picks.current?.takeUnless { it.isAnswered }
    private var choosing = false
    private val _state = MutableStateFlow<LoadingState<TrUAPIContactPickUiState>>(
        prompt?.question?.let { request ->
            LoadingState.Loaded(
                TrUAPIContactPickUiState(
                    productId = request.productId,
                    contacts = request.options.mapIndexed { index, option ->
                        TrUAPIContactPickRow(index = index, name = option.displayName)
                    }.toImmutableList(),
                )
            )
        } ?: LoadingState.Loading
    )
    override val state = _state.asStateFlow()

    init {
        prompt?.markShown()
    }

    override fun onShown() {
        if (prompt?.isAnswered != false) close()
    }

    override fun onContactClicked(index: Int) = answer {
        prompt?.let { it.answer(it.question.options.getOrNull(index)) }
    }

    override fun onDismissClicked() = answer { prompt?.dismiss() }

    override fun onCleared() {
        // The core is still blocked if the sheet went away unanswered, so a
        // dismissal has to resolve as naming nobody.
        prompt?.dismiss()
        super.onCleared()
    }

    private fun answer(choose: () -> Unit) {
        if (!choosing) {
            choosing = true
            choose()
        }
        close()
    }

    private fun close() {
        _state.value = LoadingState.Loading
        launchUnit { router.closeTrUAPIContactPick() }
    }
}
