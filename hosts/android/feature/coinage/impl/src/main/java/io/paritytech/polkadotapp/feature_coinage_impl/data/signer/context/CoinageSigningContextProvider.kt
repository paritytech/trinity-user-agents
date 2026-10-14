package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import javax.inject.Inject

interface CoinageSigningContextProvider {
    // Int type is required to get valid context
    fun freeUnloadTokenContext(period: Int, counter: Int): BandersnatchContext

    fun recyclerVouchersContext(): BandersnatchContext
}

class RealCoinageSigningContextProvider @Inject constructor() : CoinageSigningContextProvider {
    override fun freeUnloadTokenContext(period: Int, counter: Int): BandersnatchContext {
        val context = "pop:polkadot.net/coinftk".encodeToByteArray() +
            BinaryScale.encodeToByteArray(period) +
            BinaryScale.encodeToByteArray(counter)

        return BandersnatchContext(context)
    }

    override fun recyclerVouchersContext(): BandersnatchContext {
        return BandersnatchContext.fromString("pop:polkadot.network/coinrecyclr")
    }
}
