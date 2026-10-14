package io.paritytech.polkadotapp.feature_w3spay_impl.domain

import java.math.BigDecimal

/** Maximum amount accepted from the `pay-w3s` deeplink. */
const val W3S_MAX_DEEPLINK_AMOUNT = 10_000

private val W3S_AMOUNT_REGEX = Regex("^\\d+(\\.\\d{1,2})?$")

/**
 * Parses a W3S amount string: a decimal with an optional "." separator and at most two decimal
 * places. Returns `null` if the format is invalid.
 */
fun parseW3sDecimalAmount(raw: String): BigDecimal? {
    if (!W3S_AMOUNT_REGEX.matches(raw)) return null
    return raw.toBigDecimalOrNull()
}
