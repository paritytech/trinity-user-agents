package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import com.google.gson.Gson
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromByteArray
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.EraType
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getChainIdByGenesisHashOrThrow
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.toResult
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.TransactionSponsoring
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignerPayloadJson
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.Mortality
import io.paritytech.polkadotapp.feature_transactions.api.di.ExtrinsicSerializer
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import kotlinx.coroutines.withContext
import java.math.BigInteger

class SignPayloadJsonInteractor @AssistedInject constructor(
    @Assisted private val payload: SignerPayloadJson<*>,
    @Assisted private val signingSource: ProductSigningSource,
    @param:ExtrinsicSerializer private val extrinsicSerializerGson: Gson,
    private val chainRegistry: ChainRegistry,
    private val extrinsicService: ExtrinsicService,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val transactionSponsoring: TransactionSponsoring,
) : TransactionSignInteractor {
    @AssistedFactory
    interface Factory {
        fun create(payload: SignerPayloadJson<*>, signingSource: ProductSigningSource): SignPayloadJsonInteractor
    }

    override val account get() = signingSource.resolveAccount()

    override suspend fun parseSigningContent(): Result<ParsedSigningContent> = runCatching {
        val chainId = chainId()
        val runtime = chainRegistry.getRuntime(chainId)

        val call = GenericCall.fromByteArray(runtime, payload.method)

        ParsedSigningContent.Transaction(call)
    }

    override suspend fun humanReadableRepresentation(): Result<String> = runCatching {
        val parsedExtrinsic = payload.toParsedExtrinsic()
        extrinsicSerializerGson.toJson(parsedExtrinsic)
    }

    override suspend fun sign(): Result<SignedTransaction.PayloadJson> {
        return withContext(coroutineDispatchers.io) {
            prepareExtrinsicBuildingContext()
                .flatMap { context ->
                    trySponsorTransaction(context)
                        .flatMap { buildExtrinsic(context) }
                }
                .mapCatching {
                    SignedTransaction.PayloadJson(
                        signature = it.signatureHex.fromHex().toDataByteArray(),
                        signedTx = it.extrinsicHex.fromHex().toDataByteArray(),
                    )
                }
        }
    }

    private suspend fun trySponsorTransaction(context: ExtrinsicBuildingContext): Result<Unit> {
        return when (val account = signingSource.resolveAccount()) {
            is SigningAccount.Product -> transactionSponsoring.sponsorTransaction(
                chainId = context.chain.id,
                call = context.parsedExtrinsic.call,
                account = account.accountId,
            )
            // Identity-account transactions are not sponsored, and a legacy
            // account never reaches signing: the core signs those itself.
            SigningAccount.IdentityAccount, is SigningAccount.Legacy -> Result.success(Unit)
        }
    }

    private suspend fun resolveTransactionOrigin(): Result<TransactionOrigin> {
        return signingSource.createTransactionOrigin()
    }

    private suspend fun <Signer> SignerPayloadJson<Signer>.toParsedExtrinsic(): DAppParsedExtrinsic<Signer> {
        val chainId = chainId()
        val runtime = chainRegistry.getRuntime(chainId)

        val call = GenericCall.fromByteArray(runtime, method)

        return DAppParsedExtrinsic(
            account = account,
            nonce = nonce.hexBytesToBigInteger(),
            specVersion = specVersion.hexToInt(),
            transactionVersion = transactionVersion.hexToInt(),
            genesisHash = genesisHash,
            blockHash = blockHash,
            era = EraType.fromByteArray(runtime, era),
            tip = tip.hexBytesToBigInteger(),
            call = call,
            metadataHash = metadataHash,
            assetId = assetId,
        )
    }

    private suspend fun prepareExtrinsicBuildingContext(): Result<ExtrinsicBuildingContext> {
        val chainId = chainId()
        val chain = chainRegistry.getChain(chainId)

        return resolveTransactionOrigin()
            .mapCatching { origin ->
                val parsedExtrinsic = payload.toParsedExtrinsic()
                ExtrinsicBuildingContext(chain, origin, parsedExtrinsic)
            }
    }

    // Legacy payload format carries no transaction extension version, so V5 assumes 0
    private fun parseExtrinsicVersion(version: Int): Result<ExtrinsicVersion> {
        val parsed = when (version) {
            4 -> ExtrinsicVersion.V4
            5 -> ExtrinsicVersion.V5(extensionVersion = 0)
            else -> null
        }

        return parsed.toResult { "Extrinsic version $version is not supported" }
    }

    private suspend fun buildExtrinsic(context: ExtrinsicBuildingContext): Result<SendableExtrinsic> {
        val (chain, origin, parsedExtrinsic) = context

        return parseExtrinsicVersion(payload.version).flatMap { version ->
            extrinsicService.buildExtrinsic(
                chain = chain,
                origin = origin,
                options = ExtrinsicService.SubmissionOptions(
                    extrinsicVersion = version,
                    mortality = Mortality.externallyPassed(
                        parsedExtrinsic.era,
                        parsedExtrinsic.blockHash.toDataByteArray(),
                    ),
                    nonce = parsedExtrinsic.nonce,
                    specVersion = parsedExtrinsic.specVersion,
                    transactionVersion = parsedExtrinsic.transactionVersion,
                    tip = parsedExtrinsic.tip.intoBalance(),
                    metadataHash = parsedExtrinsic.metadataHash?.toDataByteArray(),
                ),
                formExtrinsic = { call(parsedExtrinsic.call) }
            )
        }
    }

    private fun ByteArray.hexBytesToBigInteger(): BigInteger {
        return BigInteger(toHexString(withPrefix = false), 16)
    }

    private fun ByteArray.hexToInt(): Int {
        return toHexString(withPrefix = false).toInt(16)
    }

    private data class ExtrinsicBuildingContext(
        val chain: Chain,
        val origin: TransactionOrigin,
        val parsedExtrinsic: DAppParsedExtrinsic<*>,
    )

    private suspend fun chainId(): ChainId {
        return chainRegistry.getChainIdByGenesisHashOrThrow(payload.genesisHash.toDataByteArray())
    }
}
