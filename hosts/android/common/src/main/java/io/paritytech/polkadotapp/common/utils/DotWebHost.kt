package io.paritytech.polkadotapp.common.utils

import uniffi.truapi.parseProductShareHost

/** Canonicalizes a product web mirror; leaves other hosts unchanged. */
fun String.toCanonicalDotHost(): String = parseProductShareHost(this)?.value ?: this
