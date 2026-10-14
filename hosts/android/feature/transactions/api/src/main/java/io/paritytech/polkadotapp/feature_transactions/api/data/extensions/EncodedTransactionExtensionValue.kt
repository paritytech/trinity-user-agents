package io.paritytech.polkadotapp.feature_transactions.api.data.extensions

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

class EncodedTransactionExtensionValue(
    val id: String,
    val implicit: DataByteArray,
    val explicit: DataByteArray,
)
