package io.paritytech.polkadotapp.feature_products_impl.domain.funding

/** A key on the funding keypad. */
sealed interface FundingKey {
    data class Digit(val digit: Char) : FundingKey

    data object Point : FundingKey

    data object Delete : FundingKey
}

/** Edits a dollar amount of at most two decimals and [maximumIntegerDigits] whole digits. */
fun String.applying(key: FundingKey, maximumIntegerDigits: Int = 7): String = when (key) {
    FundingKey.Delete -> dropLast(1)
    FundingKey.Point -> if (contains('.')) this else ifEmpty { "0" } + "."
    is FundingKey.Digit -> {
        val fraction = substringAfter('.', missingDelimiterValue = "").takeIf { contains('.') }
        when {
            fraction != null -> if (fraction.length < 2) this + key.digit else this
            this == "0" -> key.digit.toString()
            length < maximumIntegerDigits -> this + key.digit
            else -> this
        }
    }
}
