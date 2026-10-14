package io.paritytech.polkadotapp.feature_coinage_impl.data.derivation

import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock

class CoinageDerivationTest {
    private val accountRepository: AccountRepository = mock()
    private val accountSecretsStorage: AccountSecretsStorage = mock()

    private val voucherDerivation = RealVoucherRingDerivation(accountRepository, accountSecretsStorage)

    @Before
    fun setUp() = runBlocking<Unit> {
        withWalletMnemonic()
    }

    @Test
    fun `an installation page is used as the chain code unchanged`() {
        val page = decodePage("//" + TEST_INSTALLATION.asPageSegment())

        assertArrayEquals(TEST_INSTALLATION.value.value, page.chaincode)
    }

    @Test
    fun `a voucher derives along its installation's page`() = runBlocking<Unit> {
        val derived = voucherDerivation.deriveBandersnatch(CoinageKeyIndex(TEST_INSTALLATION, 7))

        assertArrayEquals(pathEntropy("//coinage-ring-vrf//4294967295//${TEST_INSTALLATION.asPageSegment()}//7"), derived.value)
    }

    @Test
    fun `the same item under two installations is two keys`() = runBlocking<Unit> {
        val other = voucherDerivation.deriveBandersnatch(CoinageKeyIndex(OTHER_INSTALLATION, 7))
        val current = voucherDerivation.deriveBandersnatch(CoinageKeyIndex(TEST_INSTALLATION, 7))

        assertNotEquals(other.value.toList(), current.value.toList())
    }

    @Test
    fun `a batch spanning installations comes back in input order`() = runBlocking<Unit> {
        val indices = listOf(
            CoinageKeyIndex(TEST_INSTALLATION, 3),
            CoinageKeyIndex(OTHER_INSTALLATION, 3),
            CoinageKeyIndex(TEST_INSTALLATION, 0),
            CoinageKeyIndex(OTHER_INSTALLATION, 9),
        )

        val batch = voucherDerivation.deriveBandersnatchBatch(indices).map { it.value.toList() }
        val oneByOne = indices.map { voucherDerivation.deriveBandersnatch(it).value.toList() }

        assertEquals(oneByOne, batch)
    }

    private fun decodePage(segment: String) = SubstrateJunctionDecoder.decode(segment).junctions.single()

    private fun pathEntropy(path: String): ByteArray {
        return SubstrateJunctionDecoder.decode(path).junctions.fold(MNEMONIC.entropy) { entropy, junction ->
            entropy.blake2b256(junction.chaincode)
        }
    }

    private suspend fun withWalletMnemonic() {
        val account: MetaAccount = mock()
        whenever(account.id).thenReturn(WALLET_ID)
        whenever(accountRepository.getWalletAccount()).thenReturn(account)
        whenever(accountSecretsStorage.getMetaAccountPassphrase(WALLET_ID)).thenReturn(MNEMONIC)
    }

    private companion object {
        const val WALLET_ID = 1L

        val OTHER_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x11 }.toDataByteArray())

        val MNEMONIC = MnemonicCreator.fromWords(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
        )
    }
}
