package io.paritytech.polkadotapp.chains.network.rpc.model

import com.google.gson.annotations.SerializedName
import io.paritytech.polkadotapp.common.utils.removeHexPrefix

class SignedBlock(val block: Block, val justification: Any?) {
    class Block(val extrinsics: List<String>, val header: Header) {
        class Header(
            @SerializedName("number")
            private val numberRaw: String,
            val parentHash: String?,
        ) {
            val number: Int
                get() {
                    return numberRaw.removeHexPrefix().toInt(radix = 16)
                }
        }
    }
}
