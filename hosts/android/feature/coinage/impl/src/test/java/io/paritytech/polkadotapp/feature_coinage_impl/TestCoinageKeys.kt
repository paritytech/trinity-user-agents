package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex

/** Deliberately not the legacy zero page, so a test relying on the installation being dropped fails loudly. */
val TEST_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x5A }.toDataByteArray())

fun testKey(item: Int) = CoinageKeyIndex(TEST_INSTALLATION, item)
