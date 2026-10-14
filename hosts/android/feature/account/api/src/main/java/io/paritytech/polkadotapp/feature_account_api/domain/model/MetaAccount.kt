package io.paritytech.polkadotapp.feature_account_api.domain.model

import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.common.domain.model.AccountId

class MetaAccount(
    val id: Long,
    val name: String,
    val signerType: SignerType,
    val purpose: Purpose,
    private val substrateCryptoType: SubstrateCryptoType,
    private val substrateAccountId: AccountId,
) {
    /**
     * Which way does the meta account use for actually signing operations
     */
    enum class SignerType {
        /**
         * Meta account signs operations using locally stored keypair
         */
        SECRETS,
    }

    enum class Purpose {
        /**
         Account should be used as a primary account in the app.
         */
        WALLET,

        /**
         * Account that is used to receive deposits
         */
        DEPOSIT,

        /**
         * Account that is used to play games
         */
        CANDIDATE,

        /**
         * Account that is used for games, etc.
         */
        ALIAS
    }

    fun accountIdIn(chain: Chain): AccountId {
        return substrateAccountId
    }

    fun defaultAccountId(): AccountId {
        return substrateAccountId
    }

    fun defaultPubKey(): AccountId {
        // For Sr25519, accountId = pubKey
        return substrateAccountId
    }

    fun addressIn(chain: Chain): String = chain.addressOf(substrateAccountId)

    fun determineEncryption(accountId: AccountId): MultiChainEncryption? {
        return if (accountId == substrateAccountId) {
            MultiChainEncryption.Substrate(substrateCryptoType.toEncryption())
        } else {
            null
        }
    }
}

fun MetaAccount.determineEncryptionOrThrow(accountId: AccountId): MultiChainEncryption {
    return requireNotNull(determineEncryption(accountId)) {
        "Could not determine encryption for accountId $accountId"
    }
}

fun MetaAccount.toOriginCaller(chain: Chain): OriginCaller {
    val accountId = accountIdIn(chain)
    return OriginCaller.System.Signed(accountId)
}
