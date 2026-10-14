package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundScreen
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.LocalFiatFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.ConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import javax.inject.Inject

@AndroidEntryPoint
class FundFragment : BaseComposeFragment<FundViewModel>() {
    @Inject
    lateinit var tokenAmountFormatter: TokenAmountFormatter

    @Inject
    lateinit var conversionFormatter: ConversionFormatter

    @Inject
    lateinit var knownTokenFormatter: KnownTokenFormatter

    @Inject
    lateinit var fiatFormatter: FiatFormatter

    @Inject
    lateinit var timeFormatter: TimeFormatter

    override val viewModel: FundViewModel by viewModels()

    @Composable
    override fun Screen() = CompositionLocalProvider(
        LocalTokenAmountFormatter provides tokenAmountFormatter,
        LocalConversionFormatter provides conversionFormatter,
        LocalKnownTokenFormatter provides knownTokenFormatter,
        LocalFiatFormatter provides fiatFormatter,
        LocalTimeFormatter provides timeFormatter
    ) {
        FundScreen(viewModel)
    }
}
