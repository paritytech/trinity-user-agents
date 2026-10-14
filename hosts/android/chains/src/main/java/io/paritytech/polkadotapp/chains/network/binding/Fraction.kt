package io.paritytech.polkadotapp.chains.network.binding

import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.toFraction
import io.paritytech.polkadotapp.common.utils.FractionUnit

fun bindFraction(decoded: Any?, unit: FractionUnit): Fraction {
    return bindNumber(decoded).toFraction(unit)
}

fun bindPermill(decoded: Any?): Fraction {
    return bindFraction(decoded, FractionUnit.PERMILL)
}

fun bindPerquintill(decoded: Any?): Fraction {
    return bindFraction(decoded, FractionUnit.PERQUINTILL)
}
