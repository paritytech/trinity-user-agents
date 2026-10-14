package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.DefaultSignedExtensions
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Era
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Extrinsic
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.Mortality
import io.paritytech.polkadotapp.feature_transactions.api.data.withMortality
import io.paritytech.polkadotapp.test_shared.whenever
import org.mockito.Mockito.mock

private const val KEY_SIZE = 32

/** A coin's on-chain key in the harness: the derivation index, so a scenario can name a coin by its number. */
fun coinKeyOf(derivationIndex: CoinageKeyIndex): AccountId = keyBytes(derivationIndex).toDataByteArray()

fun coinKeyOf(item: Int): AccountId = coinKeyOf(testKey(item))

/** A voucher's member key. Disjoint from [coinKeyOf] so the same index is two different assets. */
fun voucherKeyOf(derivationIndex: CoinageKeyIndex): BandersnatchPublicKey =
    keyBytes(derivationIndex).also { it[KEY_SIZE - 1] = 1 }.toDataByteArray()

fun voucherKeyOf(item: Int): BandersnatchPublicKey = voucherKeyOf(testKey(item))

/** The installation's leading bytes go in too, so the same item under two installations is two keys. */
private fun keyBytes(key: CoinageKeyIndex) = ByteArray(KEY_SIZE).also {
    val index = key.item
    it[0] = (index shr 24).toByte()
    it[1] = (index shr 16).toByte()
    it[2] = (index shr 8).toByte()
    it[3] = index.toByte()
    key.installation.value.value.copyInto(it, destinationOffset = 4, startIndex = 0, endIndex = 4)
}

/**
 * A fake rather than a mock: the registrar reaches derivation through top-level extensions that Mockito
 * cannot see past, and the fuzz driver derives a key on every step.
 */
class FakeCoinKeypairDerivation : CoinKeypairDerivation {
    override suspend fun deriveKeypair(derivationIndex: CoinageKeyIndex): Keypair = keypairOf(derivationIndex)

    override suspend fun deriveKeypairs(derivationIndices: List<CoinageKeyIndex>): List<Keypair> =
        derivationIndices.map(::keypairOf)

    private fun keypairOf(derivationIndex: CoinageKeyIndex) = Sr25519Keypair(
        privateKey = keyBytes(derivationIndex),
        publicKey = keyBytes(derivationIndex),
        nonce = ByteArray(KEY_SIZE),
    )
}

/**
 * Derives without the bandersnatch JNI, which has no host build. This is why [VoucherRingDerivation] exposes
 * the member key and the alias rather than the entropy they come from.
 */
class FakeVoucherRingDerivation : VoucherRingDerivation {
    override suspend fun deriveBandersnatch(derivationIndex: CoinageKeyIndex) = BandersnatchEntropy(keyBytes(derivationIndex))

    override suspend fun deriveBandersnatchBatch(derivationIndices: List<CoinageKeyIndex>) =
        derivationIndices.map { BandersnatchEntropy(keyBytes(it)) }

    var memberKeyCalls = 0
        private set
    var aliasCalls = 0
        private set

    override suspend fun memberKeyOf(derivationIndex: CoinageKeyIndex): BandersnatchPublicKey {
        memberKeyCalls++

        return voucherKeyOf(derivationIndex)
    }

    override suspend fun aliasOf(derivationIndex: CoinageKeyIndex, context: BandersnatchContext): BandersnatchAlias {
        aliasCalls++

        return aliasWithoutCounting(derivationIndex, context)
    }

    /**
     * The same derivation without touching the counters, for scenario setup.
     *
     * [aliasCalls] is evidence that production code derived an alias; a scenario computing the same key to
     * write chain state must not inflate it.
     */
    fun aliasWithoutCounting(derivationIndex: CoinageKeyIndex, context: BandersnatchContext): BandersnatchAlias =
        BandersnatchAlias(keyBytes(derivationIndex) + context.value)
}

/**
 * A signed extrinsic carrying a real `CheckMortality` era, which is what the registrar reads its window
 * from. [anchorBlock] is the block the era is built against, exactly as `MortalityConstructor` builds it.
 */
fun mortalExtrinsic(
    hex: String,
    anchorBlock: Long,
    periodBlocks: Int,
    anchorHash: String,
): EnrichedSendableExtrinsic {
    val era = Era.getEraFromBlockPeriod(anchorBlock.toInt(), periodBlocks)

    return extrinsicWithExplicits(hex, mapOf(DefaultSignedExtensions.CHECK_MORTALITY to era))
        .withMortality(Mortality(era, anchorHash.fromHex().toDataByteArray(), eraBlockNumber = anchorBlock))
}

/** No mortal era at all, so the registrar has no window to anchor to. */
fun immortalExtrinsic(hex: String): EnrichedSendableExtrinsic =
    extrinsicWithExplicits(hex, mapOf(DefaultSignedExtensions.CHECK_MORTALITY to Era.Immortal))
        .withMortality(Mortality.externallyPassed(Era.Immortal, ByteArray(32).toDataByteArray()))

/** An extrinsic this app did not build: it carries an era but cannot say which block anchors it. */
fun externallySignedExtrinsic(hex: String, anchorBlock: Long, periodBlocks: Int): EnrichedSendableExtrinsic {
    val era = Era.getEraFromBlockPeriod(anchorBlock.toInt(), periodBlocks)

    return extrinsicWithExplicits(hex, mapOf(DefaultSignedExtensions.CHECK_MORTALITY to era))
        .withMortality(Mortality.externallyPassed(era, ByteArray(32).toDataByteArray()))
}

private fun extrinsicWithExplicits(hex: String, explicits: Map<String, Any?>): SendableExtrinsic {
    val instance = Extrinsic.Instance(
        type = Extrinsic.ExtrinsicType.Signed(
            accountIdentifier = null,
            signature = null,
            signedExtras = explicits,
        ),
        call = mock(GenericCall.Instance::class.java),
    )

    return mock(SendableExtrinsic::class.java).also {
        whenever(it.extrinsicHex).thenReturn(hex)
        whenever(it.extrinsic).thenReturn(instance)
    }
}
