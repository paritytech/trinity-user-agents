package io.paritytech.polkadotapp.feature_statement_store_impl.data.extension

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.common.utils.toLittleEndianBytes
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodProductContext
import io.paritytech.polkadotapp.feature_people_api.domain.personhoodSystemSuffix

private const val STATEMENT_STORE_SLOT_FAMILY = 2u

internal fun BandersnatchContext.Companion.statementStoreSlot(
    tld: DotNsTld,
    period: UInt,
    seq: UInt
): BandersnatchContext {
    val suffix = personhoodSystemSuffix(STATEMENT_STORE_SLOT_FAMILY, period, seq.toLittleEndianBytes())

    return personhoodProductContext(tld, suffix)
}
