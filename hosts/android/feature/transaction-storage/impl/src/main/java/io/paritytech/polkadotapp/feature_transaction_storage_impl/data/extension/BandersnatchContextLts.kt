package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.extension

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodProductContext
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodSystemSuffix

private const val LONG_TERM_STORAGE_FAMILY = 3u

internal fun BandersnatchContext.Companion.longTermStorageClaim(
    tld: DotNsTld,
    period: UInt,
    counter: UByte
): BandersnatchContext {
    val suffix = personhoodSystemSuffix(LONG_TERM_STORAGE_FAMILY, period, byteArrayOf(counter.toByte()))

    return personhoodProductContext(tld, suffix)
}
