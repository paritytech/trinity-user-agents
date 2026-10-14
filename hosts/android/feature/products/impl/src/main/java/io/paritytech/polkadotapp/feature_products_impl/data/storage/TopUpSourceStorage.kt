package io.paritytech.polkadotapp.feature_products_impl.data.storage

import com.google.gson.Gson
import com.google.gson.annotations.SerializedName
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import kotlinx.coroutines.withContext
import javax.inject.Inject

private const val SOURCE_PREFIX = "TopUpSource."

/**
 * Where a top-up still in flight draws its funds from.
 *
 * Separate from the top-up itself, which the database holds, because this is the one part of it that is
 * secret: two of the three sources are bearer keys a product handed over. Encrypted, one entry per
 * operation, and removed the moment a verdict is reached — the keys can only ever build another attempt, and
 * there will not be another one.
 *
 * Never enumerated, which is why there is no index: the unfinished top-ups are a query on the database, and
 * a source is only ever fetched for one of those.
 */
interface TopUpSourceStorage {
    /** Only meaningful for a top-up that has not reached a verdict; null once one has. */
    suspend fun get(groupId: CoinageOperationGroupId): PaymentTopUpSource?

    suspend fun put(groupId: CoinageOperationGroupId, source: PaymentTopUpSource): Result<Unit>

    suspend fun remove(groupId: CoinageOperationGroupId)
}

class RealTopUpSourceStorage @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
    private val gson: Gson,
    private val dispatchers: CoroutineDispatchers,
) : TopUpSourceStorage {
    override suspend fun get(groupId: CoinageOperationGroupId): PaymentTopUpSource? = withContext(dispatchers.io) {
        val stored = encryptedPreferences.getDecryptedString(prefsKey(groupId)) ?: return@withContext null

        runCatching { gson.fromJson(stored, StoredSource::class.java).toDomain() }
            .logFailure("Failed to read the source held for ${groupId.value}")
            .getOrNull()
    }

    override suspend fun put(
        groupId: CoinageOperationGroupId,
        source: PaymentTopUpSource,
    ): Result<Unit> = withContext(dispatchers.io) {
        runCatching {
            encryptedPreferences.putEncryptedString(prefsKey(groupId), gson.toJson(source.toStored()))
        }.logFailure("Failed to hold the source for ${groupId.value}")
    }

    override suspend fun remove(groupId: CoinageOperationGroupId) = withContext(dispatchers.io) {
        encryptedPreferences.removeKey(prefsKey(groupId))
    }

    private fun prefsKey(groupId: CoinageOperationGroupId) = SOURCE_PREFIX + groupId.value
}

/**
 * The source as bytes rather than as anything resolved: a keypair and a derived signer are both built from
 * these, and rebuilding them on the way out is what lets a resumed top-up sign exactly as the first attempt
 * would have.
 */
private class StoredSource(
    val tag: SourceTag,
    val hex: String? = null,
    val listHex: List<String>? = null,
)

private enum class SourceTag {
    @SerializedName("ProductAccount")
    PRODUCT_ACCOUNT,

    @SerializedName("PrivateKey")
    PRIVATE_KEY,

    @SerializedName("Coins")
    COINS,
}

private fun PaymentTopUpSource.toStored() = when (this) {
    is PaymentTopUpSource.ProductAccount ->
        StoredSource(SourceTag.PRODUCT_ACCOUNT, hex = index.bytes.value.toHexString())

    is PaymentTopUpSource.PrivateKey -> StoredSource(SourceTag.PRIVATE_KEY, hex = key.value.toHexString())

    is PaymentTopUpSource.Coins ->
        StoredSource(SourceTag.COINS, listHex = secretKeys.map { it.value.toHexString() })
}

private fun StoredSource.toDomain(): PaymentTopUpSource = when (tag) {
    SourceTag.PRODUCT_ACCOUNT -> {
        val index = DerivationIndex32.fromBytes(requiredHex(hex)).getOrNull()

        PaymentTopUpSource.ProductAccount(requireNotNull(index) { "top-up held an unusable derivation index" })
    }

    SourceTag.PRIVATE_KEY -> PaymentTopUpSource.PrivateKey(requiredHex(hex))

    SourceTag.COINS -> PaymentTopUpSource.Coins(
        requireNotNull(listHex) { "Coins top-up held without its keys" }.map { DataByteArray(it.fromHex()) }
    )
}

private fun requiredHex(hex: String?) =
    DataByteArray(requireNotNull(hex) { "top-up held without its source" }.fromHex())
