package io.paritytech.polkadotapp.common.utils

import kotlin.random.Random

fun Random.randomBytes(size: Int): ByteArray {
    return ByteArray(size).also { nextBytes(it) }
}

fun getRandomIntExcluding(range: IntRange, excluded: List<Int>): Int {
    return range.filterNot { it in excluded }.random()
}
