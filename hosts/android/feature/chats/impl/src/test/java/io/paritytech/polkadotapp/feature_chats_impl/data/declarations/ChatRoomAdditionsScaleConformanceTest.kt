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
 * Each value is the version byte 1, then the SCALE bytes of the wire type.
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ChatRoomAdditionsScaleConformanceTest {
    @Test
    fun `decode declared features from encoded bytes`() {
        assertEquals(FEATURES_DECLARED_MODEL, FEATURES_DECLARED.fromHex().decodeFeatures())
        assertEquals(ChatRoomFeaturesModel.HOST_DEFAULT, FEATURES_DEFAULT.fromHex().decodeFeatures())
        assertEquals(FEATURES_FILES_MODEL, FEATURES_FILES.fromHex().decodeFeatures())
    }

    @Test
    fun `features encode to the frozen bytes`() {
        assertEquals(FEATURES_DECLARED, FEATURES_DECLARED_MODEL.encode().toHexString())
        assertEquals(FEATURES_DEFAULT, ChatRoomFeaturesModel.HOST_DEFAULT.encode().toHexString())
        assertEquals(FEATURES_FILES, FEATURES_FILES_MODEL.encode().toHexString())
    }

    @Test
    fun `a value of an unknown version or that does not decode reads as not declared`() {
        assertNull(("02" + FEATURES_DEFAULT.drop(2)).fromHex().decodeFeatures())
        assertNull("0101".fromHex().decodeFeatures())
    }

    private companion object {
        const val FEATURES_DECLARED = "010100010100010000"
        const val FEATURES_DEFAULT = "01000000000000"
        const val FEATURES_FILES = "01000100000101"

        val FEATURES_DECLARED_MODEL = ChatRoomFeaturesModel(
            preimageAttachments = true,
            search = false,
            calls = ChatRoomCallsModel(audio = true, video = false),
            readAloud = true,
            sealedAttachments = false,
            hopAttachments = false,
        )

        val FEATURES_FILES_MODEL = ChatRoomFeaturesModel.HOST_DEFAULT.copy(search = true, sealedAttachments = true, hopAttachments = true)
    }
}
