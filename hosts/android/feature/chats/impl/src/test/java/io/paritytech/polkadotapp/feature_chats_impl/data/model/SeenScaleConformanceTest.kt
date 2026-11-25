package io.paritytech.polkadotapp.feature_chats_impl.data.model

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import org.junit.Assert.assertEquals
import org.junit.Test

class SeenScaleConformanceTest {
    private val seenVectorHex =
        "16" +
            "90" + "30663866616435622d643963622d343639662d613136352d373038363737323839353065" +
            "a0217b6800000000"

    @Test
    fun `Seen encodes to the frozen vector`() {
        val seen: ChatMessageStatementContent = ChatMessageStatementContent.Seen(
            messageId = "0f8fad5b-d9cb-469f-a165-70867728950e",
            timestamp = 1_752_900_000u
        )

        val encoded = BinaryScale.encodeToByteArray(seen)

        assertEquals(seenVectorHex, encoded.toHexString())
    }
}
