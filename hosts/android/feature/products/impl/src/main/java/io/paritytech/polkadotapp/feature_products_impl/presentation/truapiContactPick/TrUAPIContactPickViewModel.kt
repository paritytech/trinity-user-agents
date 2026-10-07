package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIContactPicks
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class TrUAPIContactPickViewModel @Inject constructor(
    private val router: ProductsRouter,
    picks: TrUAPIContactPicks,
) : BaseViewModel(), TrUAPIContactPickContract {
    // None after Android restores the sheet in a new process, and answered when the sheet
    // appeared too late. Either way there is nobody to pick for.
    private val prompt = picks.current?.takeUnless { it.isAnswered }
    private val choosing = MutableStateFlow(false)

    init {
        prompt?.markShown()
    }

    /** Closes the sheet when it comes back on top after its prompt already ended. */
    override fun onShown() {
        if (prompt?.isAnswered != false) launchUnit { router.closeTrUAPIContactPick() }
    }

    override val state: StateFlow<LoadingState<TrUAPIContactPickUiState>> =
        choosing.map { toUiState() }
            .withLoading("TrUAPIContactPick")
            .inBackground()
            .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

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

    private fun toUiState() = TrUAPIContactPickUiState(
        productId = prompt?.question?.productId.orEmpty(),
        contacts = prompt?.question?.options.orEmpty().mapIndexed { index, option ->
            TrUAPIContactPickRow(index = index, name = option.displayName)
        }.toImmutableList(),
    )

    private fun answer(choose: () -> Unit) = launchUnit {
        if (!choosing.value) {
            choosing.value = true
            choose()
        }
        router.closeTrUAPIContactPick()
    }
}
