package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertArrayEquals
import org.junit.Test

class SsoResourceAllocationScaleTest {
    /**
     * The core decodes this as `SsoAllocatedResource::AutoSigning`: the 64-byte key then the 32-byte
     * ring-VRF domain entropy, as pinned by `auto_signing_secret_is_fixed_width_on_the_mobile_wire`.
     * A reply without the entropy is one the core cannot decode.
     */
    @Test
    fun `an AutoSigning allocation encodes as the core decodes it`() {
        val outcome: SsoApAllocationOutcomeScale = SsoApAllocationOutcomeScale.Allocated(
            SsoApAllocatedResourceScale.AutoSigning(
                productRootSecretKey = ByteArray(64) { 0x11 },
                ringVrfDomainEntropy = ByteArray(32) { 0x22 },
            ),
        )

        val expected = byteArrayOf(0x00, 0x03) + ByteArray(64) { 0x11 } + ByteArray(32) { 0x22 }
        assertArrayEquals(expected, BinaryScale.encodeToByteArray(outcome))
    }
}
