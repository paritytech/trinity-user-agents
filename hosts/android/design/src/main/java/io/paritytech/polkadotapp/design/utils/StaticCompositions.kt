package io.paritytech.polkadotapp.design.utils

fun noLocalProvidedFor(name: String): Nothing {
    error("CompositionLocal $name not present")
}
