package io.paritytech.polkadotapp.feature_coinage_impl.data.installation

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex

fun ByteArray.toCoinageInstallationId() = CoinageInstallationId(toDataByteArray())

fun ByteArray.toCoinageKeyIndex(item: Int) = CoinageKeyIndex(toCoinageInstallationId(), item)

// SQLite has no tuple IN, so lookups by key go one installation at a time.
inline fun <R> List<CoinageKeyIndex>.queryPerInstallation(query: (installationId: ByteArray, items: List<Int>) -> List<R>): List<R> {
    return groupBy { it.installation }.flatMap { (installation, indices) ->
        query(installation.value.value, indices.map { it.item })
    }
}
