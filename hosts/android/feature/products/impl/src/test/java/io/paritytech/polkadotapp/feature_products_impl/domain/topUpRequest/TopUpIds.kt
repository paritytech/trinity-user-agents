package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray

/** A well-formed id from a readable seed, so a test can name its top-ups without writing out 32 bytes. */
internal fun topUpId(seed: String): PaymentTopUpId =
    PaymentTopUpId.fromBytes(seed.encodeToByteArray().copyOf(32).toDataByteArray()).getOrThrow()
