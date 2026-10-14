package io.paritytech.polkadotapp.feature_pgas_impl.data.extension

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.common.utils.toLittleEndianBytes
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodProductContext
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodSystemSuffix

private const val PGAS_CLAIM_FAMILY = 4u

internal fun BandersnatchContext.Companion.pgasClaim(
    tld: DotNsTld,
    day: UInt,
    slot: UInt
): BandersnatchContext {
    val suffix = personhoodSystemSuffix(PGAS_CLAIM_FAMILY, day, slot.toLittleEndianBytes())

    return personhoodProductContext(tld, suffix)
}
