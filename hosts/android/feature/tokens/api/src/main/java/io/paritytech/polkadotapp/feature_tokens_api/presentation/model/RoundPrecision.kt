package io.paritytech.polkadotapp.feature_tokens_api.presentation.model

enum class RoundPrecision(val digits: Int, val minDigits: Int) {
    FIAT(digits = 2, minDigits = 2),
    DEFAULT(digits = 3, minDigits = 0),
    HIGH(digits = 5, minDigits = 0)
}
