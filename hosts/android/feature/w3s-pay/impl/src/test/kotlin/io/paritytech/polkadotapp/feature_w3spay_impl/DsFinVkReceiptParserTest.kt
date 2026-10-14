package io.paritytech.polkadotapp.feature_w3spay_impl

import io.paritytech.polkadotapp.feature_w3spay_impl.domain.dsfinvk.DsFinVkReceiptParser
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigDecimal

class DsFinVkReceiptParserTest {
    private val sample1 = "V0;1342061307;Kassenbeleg-V1;Beleg^9.00_0.00_0.00_0.00_0.00^9.00:Bar;8041;17011;" +
        "2026-05-21T21:40:56.000Z;2026-05-21T21:41:30.000Z;ecdsa-plain-SHA256;unixTime;" +
        "D5CdNgSCwlSisYXjQoZnIxAM9nRdb91F8l6uIdR+oKWi7/kTszRK/xRLBNgcGhc6L1ChYQktJJFmzKFt8mTn/w==;" +
        "BJlf238fEMG/ycfzOUBpIHa8OZNMXFMZx9ug42Vs6F0zOx42io2pnoWnRvoNeITAY1J4+2ePsszO3CeJrgfLWb8="

    private val sample2 = "V0;ERS aacbe40e-3aa6-48a1-b8e6-3c8abbd7ebd5;Kassenbeleg-V1;" +
        "Beleg^0.00_8.20_0.00_0.00_1.80^10.00:Bar;11;138;" +
        "2023-12-13T16:01:44.000Z;2023-12-13T16:01:56.000Z;ecdsa-plain-SHA256;unixTime;" +
        "7nTRzYZ6Pjhnv5l8Qo/Gs9cQ2KvmCMMK1/rQ0hoPtjkp2tPr3yuWPvH9rkOkCFuAI79k/VxvkyxwSQyWjRc7iA==;" +
        "BCC8Xaw0n2bnmcsOpLwgYhlUEw/aOvJPFMy2WOFaabktCrxep80VY7Y8KdrjIAx+9ta7wfMO03k4nwN11ZnNKm4="

    @Test
    fun `parses numeric serial sample`() {
        val receipt = DsFinVkReceiptParser.parse(sample1).getOrThrow()

        assertEquals("1342061307", receipt.serialNumber)
        assertEquals("8041", receipt.transactionNumber)
        assertEquals(0, BigDecimal("9.00").compareTo(receipt.amount))
    }

    @Test
    fun `parses serial with spaces and hyphens and sums multi-rate amount`() {
        val receipt = DsFinVkReceiptParser.parse(sample2).getOrThrow()

        assertEquals("ERS aacbe40e-3aa6-48a1-b8e6-3c8abbd7ebd5", receipt.serialNumber)
        assertEquals("11", receipt.transactionNumber)
        assertEquals(0, BigDecimal("10.00").compareTo(receipt.amount))
    }

    @Test
    fun `parses real captured sample`() {
        val captured = "V0;1342061307;Kassenbeleg-V1;Beleg^3.00_0.00_0.00_0.00_0.00^3.00:Bar;8187;17404;" +
            "2026-06-01T16:05:25.000Z;2026-06-01T16:05:37.000Z;ecdsa-plain-SHA256;unixTime;" +
            "2a11b78GnBhRc6Tu84zRsqSPfwaLwxfys4hSnX+4KK50z0VzVnMWIPNTBZfdKdonZRw7pPMH6r0OZtELhuuQrg==;" +
            "BJlf238fEMG/ycfzOUBpIHa8OZNMXFMZx9ug42Vs6F0zOx42io2pnoWnRvoNeITAY1J4+2ePsszO3CeJrgfLWb8="

        val receipt = DsFinVkReceiptParser.parse(captured).getOrThrow()

        assertEquals("1342061307", receipt.serialNumber)
        assertEquals("8187", receipt.transactionNumber)
        assertEquals(0, BigDecimal("3.00").compareTo(receipt.amount))
    }

    @Test
    fun `sums multiple payment entries after the second caret`() {
        val multiTender = "V0;1;Kassenbeleg-V1;Beleg^9.00_0.00_0.00_0.00_0.00^5.00:Bar_4.00:Unbar;7;1;" +
            "2026-05-21T21:40:56.000Z;2026-05-21T21:41:30.000Z;ecdsa-plain-SHA256;unixTime;sig==;key="

        val receipt = DsFinVkReceiptParser.parse(multiTender).getOrThrow()
        assertEquals(0, BigDecimal("9.00").compareTo(receipt.amount))
    }

    @Test
    fun `rejects non V0 prefix`() {
        assertTrue(DsFinVkReceiptParser.parse("polkadotapp://w3spay.dot/pay-w3s?id=abc").isFailure)
    }

    @Test
    fun `rejects other process types`() {
        val other = "V0;1342061307;AVTransfer;Beleg^9.00^9.00:Bar;8041;17011;" +
            "2026-05-21T21:40:56.000Z;2026-05-21T21:41:30.000Z;ecdsa-plain-SHA256;unixTime;sig==;key="
        assertTrue(DsFinVkReceiptParser.parse(other).isFailure)
    }

    @Test
    fun `rejects truncated qr without enough fields`() {
        assertTrue(DsFinVkReceiptParser.parse("V0;1342061307;Kassenbeleg-V1;Beleg^9.00^9.00:Bar").isFailure)
    }
}
