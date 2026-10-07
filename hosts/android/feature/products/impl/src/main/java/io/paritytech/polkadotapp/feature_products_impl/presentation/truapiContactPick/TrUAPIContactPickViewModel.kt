package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickOption
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt
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
    private val prompt: TrUAPIPrompt<ContactPickRequest, ContactPickOption?>,
) : BaseViewModel(), TrUAPIContactPickContract {
    private val choosing = MutableStateFlow(false)

    init {
        prompt.markShown()
    }

    override val state: StateFlow<LoadingState<TrUAPIContactPickUiState>> =
        choosing.map { toUiState() }
            .withLoading("TrUAPIContactPick")
            .inBackground()
            .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onContactClicked(index: Int) = answer {
        prompt.answer(prompt.question.options.getOrNull(index))
    }

    override fun onDismissClicked() = answer { prompt.dismiss() }

    override fun onCleared() {
        // The core is still blocked if the sheet went away unanswered, so a
        // dismissal has to resolve as naming nobody.
        prompt.dismiss()
        super.onCleared()
    }

    private fun toUiState() = TrUAPIContactPickUiState(
        productId = prompt.question.productId,
        contacts = prompt.question.options.mapIndexed { index, option ->
            TrUAPIContactPickRow(index = index, name = option.displayName)
        }.toImmutableList(),
    )

    private fun answer(choose: () -> Unit) = launchUnit {
        if (choosing.value) return@launchUnit
        choosing.value = true
        choose()
        router.back()
    }
}
