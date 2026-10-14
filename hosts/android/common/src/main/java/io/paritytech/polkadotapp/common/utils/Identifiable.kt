package io.paritytech.polkadotapp.common.utils

import androidx.annotation.Keep

@Keep
interface Identifiable {
    val identifier: String
}

fun <T : Identifiable> Iterable<T>.findById(other: Identifiable?): T? = find { it.identifier == other?.identifier }

fun <T : Identifiable> Iterable<T>.findById(id: String): T? = find { it.identifier == id }

fun <T : Identifiable> Iterable<T>.firstById(id: String): T = first { it.identifier == id }
