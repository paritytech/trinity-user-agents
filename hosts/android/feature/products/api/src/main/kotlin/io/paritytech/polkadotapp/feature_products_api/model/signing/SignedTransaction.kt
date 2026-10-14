package io.paritytech.polkadotapp.feature_products_api.model.signing

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature

sealed interface SignedTransaction {
    sealed interface WithDedicatedSignature

    class GeneralTransaction(val signedTx: DataByteArray) : SignedTransaction

    class PayloadJson(val signature: DataByteArray, val signedTx: DataByteArray) : SignedTransaction, WithDedicatedSignature

    class Raw(val signature: DataByteArray) : SignedTransaction, WithDedicatedSignature

    class Vrf(val signature: VrfSignature) : SignedTransaction
}
