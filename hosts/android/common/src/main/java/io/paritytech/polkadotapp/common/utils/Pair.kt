package io.paritytech.polkadotapp.common.utils

inline fun <T, R> Pair<T, T>.transformPair(transform: (T) -> R): Pair<R, R> {
    return Pair(
        transform(first),
        transform(second)
    )
}
