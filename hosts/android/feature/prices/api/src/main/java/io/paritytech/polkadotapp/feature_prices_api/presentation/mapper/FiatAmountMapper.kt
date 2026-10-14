package io.paritytech.polkadotapp.feature_prices_api.presentation.mapper

import io.paritytech.polkadotapp.feature_prices_api.domain.model.FiatAmount
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel

interface FiatAmountMapper {
    fun mapToUi(fiatAmount: FiatAmount): FiatAmountModel
}
