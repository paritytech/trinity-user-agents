package io.paritytech.polkadotapp.feature_tokens_api.presentation.model

sealed interface TokenSymbolAppearance {
    /** The payment asset; its symbol is resolved by the formatter from the published brand. */
    object DigitalDollar : TokenSymbolAppearance

    class Symbol(val symbol: String) : TokenSymbolAppearance
}
