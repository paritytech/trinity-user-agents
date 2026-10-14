package io.paritytech.polkadotapp.common.utils

@JvmInline
value class Millimeters(val value: Int)

val Int.millimeters: Millimeters
    get() = Millimeters(this)
