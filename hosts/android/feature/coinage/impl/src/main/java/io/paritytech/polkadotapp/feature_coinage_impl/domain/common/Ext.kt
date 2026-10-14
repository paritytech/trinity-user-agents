package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

internal fun Int?.getNextIndex() = (this ?: -1) + 1
