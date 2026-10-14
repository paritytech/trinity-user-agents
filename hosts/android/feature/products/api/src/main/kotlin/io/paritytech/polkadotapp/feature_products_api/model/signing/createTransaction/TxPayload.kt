package io.paritytech.polkadotapp.feature_products_api.model.signing.createTransaction

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.EncodedTransactionExtensionValue

class TxPayload<Signer>(
    val signer: Signer,
    val genesisHash: DataByteArray,
    val callData: DataByteArray,
    val extensions: List<EncodedTransactionExtensionValue>,
    val txExtVersion: UByte,
)
