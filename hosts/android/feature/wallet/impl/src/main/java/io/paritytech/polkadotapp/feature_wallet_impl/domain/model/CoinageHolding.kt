package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHolding

/** Rows from two installations can share an item number, so both halves of the key are read. */
val holdingKeyOrder: Comparator<CoinageHolding> =
    DataByteArray.compareByBytes<CoinageHolding>(unsigned = true) { it.keyIndex.installation.value.value }
        .thenBy { it.keyIndex.item }
