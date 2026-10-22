package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Backward compatibility of the stored values of the Chat Extension v2 additions: `chat_room_declarations.features`.
 * Each value is a version byte, then the SCALE bytes of the wire type of that version.
 *
 * DO NOT modify the encoded hex values below: they are the frozen formats that must always decode.
 */
class ChatRoomAdditionsScaleConformanceTest {
    @Test
    fun `decode the features that the first version stored`() {
        assertEquals(FEATURES_DECLARED_MODEL, V1_FEATURES_DECLARED.fromHex().decodeFeatures())
        assertEquals(ChatRoomFeaturesModel.HOST_DEFAULT, V1_FEATURES_DEFAULT.fromHex().decodeFeatures())
        assertEquals(FEATURES_FILES_MODEL, V1_FEATURES_FILES.fromHex().decodeFeatures())
    }

    @Test
    fun `features encode to the frozen bytes of the second version and decode back`() {
        assertEquals(V2_FEATURES_DECLARED, FEATURES_DECLARED_MODEL.encode().toHexString())
        assertEquals(V2_FEATURES_DEFAULT, ChatRoomFeaturesModel.HOST_DEFAULT.encode().toHexString())
        assertEquals(V2_FEATURES_FILES, FEATURES_FILES_MODEL.encode().toHexString())

        assertEquals(FEATURES_DECLARED_MODEL, V2_FEATURES_DECLARED.fromHex().decodeFeatures())
        assertEquals(FEATURES_FILES_MODEL, V2_FEATURES_FILES.fromHex().decodeFeatures())
    }

    @Test
    fun `a value of an unknown version or that does not decode reads as not declared`() {
        assertNull(("03" + V2_FEATURES_DEFAULT.drop(2)).fromHex().decodeFeatures())
        assertNull("0101".fromHex().decodeFeatures())
    }

    private companion object {
        const val V1_FEATURES_DECLARED = "010100010100010000"
        const val V1_FEATURES_DEFAULT = "01000000000000"
        const val V1_FEATURES_FILES = "01000100000101"

        const val V2_FEATURES_DECLARED = "0201010000"
        const val V2_FEATURES_DEFAULT = "020000"
        const val V2_FEATURES_FILES = "020001"

        val FEATURES_DECLARED_MODEL = ChatRoomFeaturesModel(calls = ChatRoomCallsModel(audio = true, video = false), hopAttachments = false)

        val FEATURES_FILES_MODEL = ChatRoomFeaturesModel.HOST_DEFAULT.copy(hopAttachments = true)
    }
}
