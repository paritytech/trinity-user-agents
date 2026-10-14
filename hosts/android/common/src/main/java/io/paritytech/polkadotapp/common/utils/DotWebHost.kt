package io.paritytech.polkadotapp.common.utils

val DOT_WEB_MIRROR_SUFFIXES = listOf(".dot.li", ".paseo.li", ".test.li")

private const val MIRROR_ZONE_SUFFIX = ".li"

fun isDotWebMirrorHost(host: String): Boolean =
    DOT_WEB_MIRROR_SUFFIXES.any { host.endsWith(it) }

/**
 * Canonicalizes a web-mirror host to the dotNS name of its own root
 * (`coinflip.paseo.li` -> `coinflip.paseo`); leaves other hosts unchanged.
 */
fun String.toCanonicalDotHost(): String {
    if (!isDotWebMirrorHost(this)) return this
    return removeSuffix(MIRROR_ZONE_SUFFIX)
}
