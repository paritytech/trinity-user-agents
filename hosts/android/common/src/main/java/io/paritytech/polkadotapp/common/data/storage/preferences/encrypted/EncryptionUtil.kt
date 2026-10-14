package io.paritytech.polkadotapp.common.data.storage.preferences.encrypted

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import org.bouncycastle.util.Arrays
import org.bouncycastle.util.encoders.Base64
import timber.log.Timber
import java.math.BigInteger
import java.security.*
import java.security.spec.MGF1ParameterSpec
import java.util.Calendar
import javax.crypto.*
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.OAEPParameterSpec
import javax.crypto.spec.PSource
import javax.crypto.spec.SecretKeySpec
import javax.inject.Inject
import javax.inject.Singleton
import javax.security.auth.x500.X500Principal

@Singleton
class EncryptionUtil @Inject constructor(
    @ApplicationContext private val context: Context
) {
    companion object {
        private const val RSA = "RSA"
        private const val AES = "AES"
        private const val KEY_STORE_PROVIDER = "AndroidKeyStore"
        private const val TRANSFORMATION = "RSA/ECB/OAEPWITHSHA-256ANDMGF1PADDING"
        private const val KEY_ALIAS = "key_alias"
        private const val MD_NAME = "SHA-256"
        private const val MGF_NAME = "MGF1"
        private const val BLOCK_SIZE = 16
        private const val AES_KEY_LENGTH = 256
        private var second = false

        private var privateKey: PrivateKey? = null
        private var publicKey: PublicKey? = null

        private const val SECRET_KEY = "secret_key"
        private val secureRandom = SecureRandom()
        private var keyStore: KeyStore? = null
    }

    init {
        initKeystore()
    }

    private val oaepParam = OAEPParameterSpec(MD_NAME, MGF_NAME, MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT)

    // Every encrypt and decrypt resolved this, and each resolution ran an RSA
    // private-key operation in the AndroidKeyStore to unwrap the AES key. The
    // key is generated once per install and never rotated, so it is read once.
    private val preferenceAesKey: Key by lazy { loadPreferenceAesKey() }

    fun getPrerenceAesKey(): Key = preferenceAesKey

    private fun loadPreferenceAesKey(): Key {
        val secretKey: SecretKey
        val encryptedKey =
            context.getSharedPreferences(KEY_ALIAS, Context.MODE_PRIVATE).getString(
                SECRET_KEY,
                "",
            )
        if (encryptedKey!!.isEmpty()) {
            val keyGenerator = KeyGenerator.getInstance(AES)
            keyGenerator.init(AES_KEY_LENGTH, secureRandom)
            secretKey = keyGenerator.generateKey()
            context.getSharedPreferences(KEY_ALIAS, Context.MODE_PRIVATE)
                .edit { putString(SECRET_KEY, encryptRsa(secretKey.encoded)) }
        } else {
            val key = decryptRsa(encryptedKey)
            secretKey = SecretKeySpec(key, 0, key!!.size, AES)
        }
        return secretKey
    }

    private fun initKeystore() {
        try {
            keyStore = KeyStore.getInstance(KEY_STORE_PROVIDER)
            keyStore!!.load(null)

            if (keyStore!!.getKey(KEY_ALIAS, null) == null) {
                createKeys()
            }

            privateKey = keyStore!!.getKey(KEY_ALIAS, null) as PrivateKey
            publicKey = keyStore!!.getCertificate(KEY_ALIAS).publicKey
        } catch (e: Exception) {
            if (!second) {
                second = true
                initKeystore()
            }
            Timber.e(e)
        }
    }

    private fun createKeys() {
        val startDate = Calendar.getInstance()
        val endDate = Calendar.getInstance()
        endDate.add(Calendar.YEAR, 25)

        val spec =
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_ECB)
                .setDigests(KeyProperties.DIGEST_SHA256)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP)
                .setCertificateSubject(X500Principal("CN=paritytech"))
                .setCertificateSerialNumber(BigInteger.ONE)
                .setCertificateNotBefore(startDate.time)
                .setCertificateNotAfter(endDate.time)
                .build()

        val keyPairGenerator = KeyPairGenerator.getInstance(RSA, KEY_STORE_PROVIDER)
        keyPairGenerator.initialize(spec)
        keyPairGenerator.generateKeyPair()
    }

    fun encrypt(cleartext: String?): String {
        if (!cleartext.isNullOrEmpty()) {
            try {
                return encrypt(getPrerenceAesKey().encoded, cleartext)
            } catch (e: NoSuchAlgorithmException) {
                Timber.e(e)
            }
        }
        return ""
    }

    fun encrypt(
        key: ByteArray,
        cleartext: String,
    ): String {
        try {
            val result = encrypt(key, cleartext.toByteArray())
            return Base64.toBase64String(result)
        } catch (e: Exception) {
            Timber.e(e)
        }
        return ""
    }

    fun decrypt(encryptedBase64: String): String {
        try {
            return decrypt(getPrerenceAesKey().encoded, encryptedBase64)
        } catch (e: NoSuchAlgorithmException) {
            Timber.e(e)
        }

        return ""
    }

    fun decrypt(
        key: ByteArray,
        encryptedBase64: String,
    ): String {
        try {
            val encrypted = Base64.decode(encryptedBase64)
            val result = decrypt(key, encrypted)
            return String(result)
        } catch (e: Exception) {
            Timber.e(e)
        }

        return ""
    }

    @Throws(Exception::class)
    private fun encrypt(
        key: ByteArray,
        clear: ByteArray,
    ): ByteArray {
        val skeySpec = SecretKeySpec(key, "AES")
        val cipher = Cipher.getInstance("AES/CBC/PKCS5Padding")
        cipher.init(Cipher.ENCRYPT_MODE, skeySpec, IvParameterSpec(generateIVBytes()), secureRandom)
        return Arrays.concatenate(cipher.iv, cipher.doFinal(clear))
    }

    @Throws(
        NoSuchPaddingException::class,
        NoSuchAlgorithmException::class,
        InvalidAlgorithmParameterException::class,
        InvalidKeyException::class,
        BadPaddingException::class,
        IllegalBlockSizeException::class,
    )
    private fun decrypt(
        key: ByteArray,
        encrypted: ByteArray,
    ): ByteArray {
        val cipher = Cipher.getInstance("AES/CBC/PKCS5Padding")
        cipher.init(
            Cipher.DECRYPT_MODE,
            SecretKeySpec(key, "AES"),
            IvParameterSpec(Arrays.copyOfRange(encrypted, 0, BLOCK_SIZE)),
            secureRandom,
        )
        return cipher.doFinal(Arrays.copyOfRange(encrypted, BLOCK_SIZE, encrypted.size))
    }

    private fun encryptRsa(input: ByteArray): String {
        val cipher: Cipher

        try {
            cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, publicKey, oaepParam)
            return Base64.toBase64String(cipher.doFinal(input))
        } catch (e: NoSuchAlgorithmException) {
            Timber.e(e)
        } catch (e: NoSuchPaddingException) {
            Timber.e(e)
        } catch (e: InvalidKeyException) {
            Timber.e(e)
        } catch (e: BadPaddingException) {
            Timber.e(e)
        } catch (e: IllegalBlockSizeException) {
            Timber.e(e)
        }

        return ""
    }

    private fun decryptRsa(encrypted: String): ByteArray? {
        val cipher: Cipher

        try {
            cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, privateKey, oaepParam)
            return cipher.doFinal(Base64.decode(encrypted))
        } catch (e: NoSuchAlgorithmException) {
            Timber.e(e)
        } catch (e: NoSuchPaddingException) {
            Timber.e(e)
        } catch (e: InvalidKeyException) {
            Timber.e(e)
        } catch (e: BadPaddingException) {
            Timber.e(e)
        } catch (e: IllegalBlockSizeException) {
            Timber.e(e)
        }

        return null
    }

    private fun generateIVBytes(): ByteArray {
        val ivBytes = ByteArray(BLOCK_SIZE)
        secureRandom.nextBytes(ivBytes)
        return ivBytes
    }
}
