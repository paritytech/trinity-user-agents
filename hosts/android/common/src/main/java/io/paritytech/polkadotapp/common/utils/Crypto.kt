package io.paritytech.polkadotapp.common.utils

import android.util.Base64
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import okio.ByteString.Companion.decodeBase64
import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.security.MessageDigest
import java.security.Security

fun emptyEthereumAccountId() = ByteArray(20) { 1 }.intoAccountId()

fun emptySubstrateAccountId() = ByteArray(32).intoAccountId()

fun ByteArray.substrateAccountId(): AccountId {
    val result = if (size > 32) {
        this.blake2b256()
    } else {
        this
    }

    return result.intoAccountId()
}

fun String.md5(): String {
    return encodeToByteArray().md5()
}

fun ByteArray.md5(): String {
    val hasher = MessageDigest.getInstance("MD5")
    return hasher.digest(this).decodeToString()
}

fun ByteArray.sha256UrlSafe() = Base64.encode(sha256(), Base64.URL_SAFE).decodeToString()

fun ByteArray.base64NoWrap() = Base64.encode(this, Base64.NO_WRAP).decodeToString()

fun ByteArray.base64UrlSafe() = Base64.encode(this, Base64.URL_SAFE).decodeToString()

fun String.decodeFormBase64UrlSafe(): ByteArray = Base64.decode(this, Base64.URL_SAFE)

fun ByteArray.sha256(): ByteArray {
    val digest = MessageDigest.getInstance("SHA-256")
    return digest.digest(this)
}

fun String.decodeBase64toByteArray(): Result<ByteArray> {
    val result = decodeBase64()?.toByteArray()
    return if (result != null) Result.success(result) else Result.failure(Throwable("Cannot decode string"))
}

fun requireBouncyCastle() {
    val existingProvider = Security.getProvider(BouncyCastleProvider.PROVIDER_NAME)

    if (existingProvider == null ||
        existingProvider::class.qualifiedName != BouncyCastleProvider::class.qualifiedName
    ) {
        Security.removeProvider(BouncyCastleProvider.PROVIDER_NAME)
        Security.addProvider(BouncyCastleProvider())
    }
}
