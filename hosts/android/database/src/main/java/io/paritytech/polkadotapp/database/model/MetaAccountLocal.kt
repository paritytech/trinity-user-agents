package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

@Entity(
    tableName = "meta_accounts",
    indices = [
        Index(value = ["substrateAccountId"]),
    ]
)
class MetaAccountLocal(
    val substratePublicKey: ByteArray,
    val substrateCryptoType: SubstrateCryptoTypeLocal,
    val substrateAccountId: ByteArray,
    val name: String,
    val signerType: SignerTypeLocal,
    val purpose: PurposeLocal,
    val aliasContext: ByteArray?
) {
    @PrimaryKey(autoGenerate = true)
    var id: Long = 0

    enum class SignerTypeLocal {
        SECRETS,
    }

    enum class PurposeLocal {
        WALLET,

        DEPOSIT,

        ALIAS,

        CANDIDATE
    }

    enum class SubstrateCryptoTypeLocal {
        SR25519,
        ED25519,
        ECDSA,
    }
}
