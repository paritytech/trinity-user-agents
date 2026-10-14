package io.paritytech.polkadotapp.tools_integrity_impl.data.claim

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WidevineSecurityLevelTest {
    @Test
    fun `level is eligible when security property reports exact L1`() {
        assertTrue(WidevineEvidenceReader.isL1SecurityLevel("L1"))
    }

    @Test
    fun `level is ineligible when security property is not exact L1`() {
        listOf("L2", "L3", "", "l1", " L1", "L1 ", "unknown").forEach { level ->
            assertFalse("expected $level to be ineligible", WidevineEvidenceReader.isL1SecurityLevel(level))
        }
    }
}
