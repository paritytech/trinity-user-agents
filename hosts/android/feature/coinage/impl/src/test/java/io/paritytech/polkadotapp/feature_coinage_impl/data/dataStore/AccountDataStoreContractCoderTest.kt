package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pinned against ethers v6 encoding the contract's own ABI (`abi/AccountDataStore.json`). */
class AccountDataStoreContractCoderTest {
    @Test
    fun `registering encodes the record as dynamic bytes after the selector`() {
        val encoded = AccountDataStoreContractCoder.encodeRegisterInstallation(RECORD.hexToDataByteArray())

        assertEquals(REGISTER_CALL, encoded.toString())
    }

    @Test
    fun `the installation list is asked for by address`() {
        val encoded = AccountDataStoreContractCoder.encodeGetInstallations(OWNER.hexToDataByteArray())

        assertEquals(GET_CALL, encoded.toString())
    }

    @Test
    fun `a returned list of records decodes in order`() {
        val decoded = AccountDataStoreContractCoder.decodeGetInstallations(RETURNED_LIST.hexToDataByteArray())

        assertEquals(listOf(RECORD.hexToDataByteArray(), "0x0102".hexToDataByteArray()), decoded)
    }

    @Test
    fun `an empty list decodes to no records`() {
        assertTrue(AccountDataStoreContractCoder.decodeGetInstallations(RETURNED_EMPTY.hexToDataByteArray()).isEmpty())
    }

    private companion object {
        const val RECORD = "0xb3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e80acfc55b8155" +
            "9e511662b8d79151ac504ca50de705fe6736054d586"

        const val OWNER = "0x1111111111111111111111111111111111111111"

        const val REGISTER_CALL = "0xe561868d" +
            "0000000000000000000000000000000000000000000000000000000000000020" +
            "000000000000000000000000000000000000000000000000000000000000003c" +
            "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e" +
            "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000"

        const val GET_CALL = "0x740204c60000000000000000000000001111111111111111111111111111111111111111"

        const val RETURNED_LIST = "0x" +
            "0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000002" +
            "0000000000000000000000000000000000000000000000000000000000000040" +
            "00000000000000000000000000000000000000000000000000000000000000a0" +
            "000000000000000000000000000000000000000000000000000000000000003c" +
            "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e" +
            "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000" +
            "0000000000000000000000000000000000000000000000000000000000000002" +
            "0102000000000000000000000000000000000000000000000000000000000000"

        const val RETURNED_EMPTY = "0x" +
            "0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000000"
    }
}
