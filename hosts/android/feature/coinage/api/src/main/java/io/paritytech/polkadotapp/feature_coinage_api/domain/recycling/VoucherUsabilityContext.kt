package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import kotlin.time.Instant

interface VoucherUsabilityContext {
    companion object;

    val now: Instant

    fun capacityFor(recyclerValue: ValueExponent): Int
}
