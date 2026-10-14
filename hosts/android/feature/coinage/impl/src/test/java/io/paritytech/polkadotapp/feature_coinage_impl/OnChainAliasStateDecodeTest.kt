package io.paritytech.polkadotapp.feature_coinage_impl

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.Struct
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainLockReason
import junit.framework.TestCase.assertEquals
import junit.framework.TestCase.assertTrue
import org.junit.Test
import java.math.BigInteger

/**
 * Decodes through the real by-NAME metadata Scale path used for storage values, matching how
 * `RecyclerAliasStates` entries are decoded in production (DictEnum.Entry for the variant,
 * Struct.Instance for the LockInfo payload).
 */
class OnChainAliasStateDecodeTest {
    @Test
    fun `decodes Unloaded variant`() {
        val decoded: OnChainAliasState = Scale.decode(DictEnum.Entry(name = "Unloaded", value = null))

        assertTrue(decoded is OnChainAliasState.Unloaded)
    }

    @Test
    fun `decodes Locked variant with LockInfo payload`() {
        val lockInfo = Struct.Instance(
            mapping = mapOf(
                "reason" to DictEnum.Entry(
                    name = "FailedDispatch",
                    value = Struct.Instance(mapping = mapOf("retries" to BigInteger.valueOf(3)))
                ),
                "until" to BigInteger.valueOf(1_700_000_000),
            )
        )

        val decoded: OnChainAliasState = Scale.decode(DictEnum.Entry(name = "Locked", value = lockInfo))

        assertTrue(decoded is OnChainAliasState.Locked)
        val locked = decoded as OnChainAliasState.Locked
        assertEquals(BigInteger.valueOf(1_700_000_000), locked.until)
        assertTrue(locked.reason is OnChainLockReason.FailedDispatch)
        assertEquals(3, (locked.reason as OnChainLockReason.FailedDispatch).retries)
    }
}
