package io.paritytech.polkadotapp.feature_prices_impl.presentation.mapper

import io.paritytech.polkadotapp.feature_prices_api.domain.model.FiatAmount
import io.paritytech.polkadotapp.feature_prices_api.domain.model.display
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.FiatAmountMapper
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel
import javax.inject.Inject

class RealFiatAmountMapper @Inject constructor() : FiatAmountMapper {
    override fun mapToUi(fiatAmount: FiatAmount): FiatAmountModel {
        return FiatAmountModel(
            fiatAmount = fiatAmount.amountPrice,
            currencyDisplay = fiatAmount.currency.display
        )
    }
}
