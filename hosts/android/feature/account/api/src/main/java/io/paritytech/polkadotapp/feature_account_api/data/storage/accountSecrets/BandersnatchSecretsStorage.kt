package io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchCrypto
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray

interface BandersnatchSecretsStorage {
    suspend fun getEntropy(metaId: Long): BandersnatchEntropy
}

suspend fun BandersnatchSecretsStorage.getMemberKey(metaId: Long): BandersnatchPublicKey {
    return BandersnatchCrypto.derive_member_key(getEntropy(metaId).value).toDataByteArray()
}

suspend fun BandersnatchSecretsStorage.sign(
    metaId: Long,
    message: ByteArray
): ByteArray {
    return BandersnatchCrypto.sign(getEntropy(metaId).value, message)
}

suspend fun BandersnatchSecretsStorage.getAliasInContext(
    metaId: Long,
    context: BandersnatchContext
): BandersnatchAlias {
    return getEntropy(metaId).aliasInContext(context)
}

suspend fun BandersnatchSecretsStorage.getContextualAlias(
    metaId: Long,
    context: BandersnatchContext
): ContextualAlias {
    val alias = getEntropy(metaId).aliasInContext(context)
    return ContextualAlias(context, alias)
}
