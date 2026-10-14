package io.paritytech.polkadotapp.feature_account_api.data.sign

import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.MultiSignature
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

interface AccountBytesSigner {
    /**
     * Sign unprocessed, raw signature with currently selected MetaAccount
     * Make sure to use it only with trusted, non-external input
     */
    suspend fun signRawBytesByWallet(
        message: ByteArray,
        chainId: ChainId,
        context: MessageSigningContext
    ): Result<SignatureWrapper>

    /**
     * Sign unprocessed, raw signature with candidate MetaAccount
     * Make sure to use it only with trusted, non-external input
     */
    suspend fun signRawBytesByCandidate(message: ByteArray, context: MessageSigningContext): MultiSignature

    suspend fun signRawBytes(message: ByteArray, context: MessageSigningContext, account: MetaAccount): MultiSignature

    suspend fun signWithBandersnatchByWallet(message: ByteArray, context: MessageSigningContext): Result<ByteArray>
}
