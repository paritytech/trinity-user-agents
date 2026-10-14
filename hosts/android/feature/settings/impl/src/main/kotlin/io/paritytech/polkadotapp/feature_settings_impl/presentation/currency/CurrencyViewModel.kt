package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency

import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CanadianDollar
import io.paritytech.polkadotapp.design.components.icon.vectors.Dollar
import io.paritytech.polkadotapp.design.components.icon.vectors.Euro
import io.paritytech.polkadotapp.design.components.icon.vectors.IndonesianRupiah
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.CurrencyInteractor
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyState
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyUiModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class CurrencyViewModel @Inject constructor(
    private val router: SettingsRouter,
    private val interactor: CurrencyInteractor,
) : BaseViewModel(), CurrencyContract {
    companion object {
        private const val STOP_TIMEOUT_MILLIS = 5000L
    }

    private val supportedCurrenciesFlow: Flow<List<Currency>> = flow {
        emit(interactor.getSupportedCurrencies())
    }

    private val availableCurrenciesUiFlow: Flow<List<CurrencyUiModel>> =
        supportedCurrenciesFlow.map { list ->
            list.map { it.toUiModel() }
        }

    override val state: StateFlow<CurrencyState> =
        interactor.selectedCurrencyFlow()
            .combine(availableCurrenciesUiFlow) { selectedDomain, availableUi ->

                CurrencyState(
                    selectedCurrency = selectedDomain.toUiModel(),
                    availableCurrencies = availableUi
                )
            }
            .stateIn(
                scope = viewModelScope,
                started = SharingStarted.WhileSubscribed(STOP_TIMEOUT_MILLIS),
                initialValue = CurrencyState()
            )

    override fun onBackClick() {
        router.back()
    }

    override fun onCurrencySelected(currency: CurrencyUiModel) {
        if (currency.code == state.value.selectedCurrency?.code) return

        viewModelScope.launch {
            val domainCurrency = interactor.getSupportedCurrencies()
                .firstOrNull { it.code == currency.code }
                ?: return@launch

            interactor.updateSelectedCurrency(domainCurrency)

            delay(200)
            router.back()
        }
    }

    private fun Currency.toUiModel(): CurrencyUiModel {
        val icon = when (this) {
            Currency.USD -> NovaIcons.Dollar
            Currency.EUR -> NovaIcons.Euro
            Currency.CAD -> NovaIcons.CanadianDollar
            Currency.IDR -> NovaIcons.IndonesianRupiah
        }

        return CurrencyUiModel(
            code = code,
            name = displayName,
            icon = icon
        )
    }
}
