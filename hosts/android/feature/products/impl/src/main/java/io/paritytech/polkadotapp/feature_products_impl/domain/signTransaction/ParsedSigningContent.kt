package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall

sealed interface ParsedSigningContent {
    class Transaction(val call: GenericCall.Instance) : ParsedSigningContent
    object Raw : ParsedSigningContent
    class Vrf(val transcriptLabel: String) : ParsedSigningContent
}
