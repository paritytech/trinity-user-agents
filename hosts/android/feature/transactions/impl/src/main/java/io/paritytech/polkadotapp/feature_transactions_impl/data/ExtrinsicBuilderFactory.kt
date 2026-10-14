package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.novasama.substrate_sdk_android.runtime.extrinsic.BatchMode
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.Nonce
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.checkMetadataHash.CheckMetadataHashMode
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.GeneralTransactionSigner
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.chain.RuntimeVersion
import io.paritytech.polkadotapp.chains.mapper.toRuntimeVersion
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.feature_transactions.api.data.Mortality
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.ChargeAssetTxPayment.Companion.chargeAssetTxPayment
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.FeeTransactionSigner
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SubmissionTransactionSigner
import io.paritytech.polkadotapp.feature_transactions_impl.data.ExtrinsicBuilderFactory.Options
import java.math.BigInteger
import javax.inject.Inject
import javax.inject.Singleton

interface ExtrinsicBuilderFactory {
    class Options(
        val batchMode: BatchMode,
        val extrinsicVersion: ExtrinsicVersion?,
        val mortality: Mortality?,
        val nonce: Nonce?,
        val tip: Balance?,
        val metadataHash: ByteArray?,
        val transactionVersion: Int?,
        val specVersion: Int?,
    )

    suspend fun createForFee(
        chain: Chain,
        signer: FeeTransactionSigner,
        options: Options,
    ): ExtrinsicBuilder

    suspend fun createForSubmission(
        chain: Chain,
        signer: SubmissionTransactionSigner?,
        requestedSignerAccountId: AccountId?,
        options: Options,
    ): Submittable

    suspend fun createMultiForSubmission(
        chain: Chain,
        signer: SubmissionTransactionSigner?,
        requestedSignerAccountId: AccountId?,
        options: Options,
    ): Sequence<Submittable>

    /**
     * A builder together with the [Mortality] it was configured with. Resolving mortality is this factory's
     * job, but the anchor is not recoverable from the signed bytes, so a caller that must record it needs it
     * reported back rather than re-derived.
     */
    class Submittable(val builder: ExtrinsicBuilder, val mortality: Mortality)
}

@Singleton
class RealExtrinsicBuilderFactory @Inject constructor(
    private val chainDao: ChainDao,
    private val rpcCalls: RpcCalls,
    private val chainRegistry: ChainRegistry,
    private val mortalityConstructor: MortalityConstructor,
    private val extrinsicVersionProvider: DefaultExtrinsicVersionProvider
) : ExtrinsicBuilderFactory {
    override suspend fun createForFee(
        chain: Chain,
        signer: FeeTransactionSigner,
        options: Options
    ): ExtrinsicBuilder {
        return createMultiForFee(signer, chain, options).first().builder
    }

    /**
     * Create with real keypair
     */
    override suspend fun createForSubmission(
        chain: Chain,
        signer: SubmissionTransactionSigner?,
        requestedSignerAccountId: AccountId?,
        options: Options,
    ): ExtrinsicBuilderFactory.Submittable {
        return createMulti(chain, signer, requestedSignerAccountId, options).first()
    }

    override suspend fun createMultiForSubmission(
        chain: Chain,
        signer: SubmissionTransactionSigner?,
        requestedSignerAccountId: AccountId?,
        options: Options
    ): Sequence<ExtrinsicBuilderFactory.Submittable> {
        return createMulti(chain, signer, requestedSignerAccountId, options)
    }

    private suspend fun createMultiForFee(
        signer: FeeTransactionSigner,
        chain: Chain,
        options: Options
    ): Sequence<ExtrinsicBuilderFactory.Submittable> {
        return createMulti(chain, signer, signer.fakeSignerId(chain), options)
    }

    private suspend fun createMulti(
        chain: Chain,
        signer: GeneralTransactionSigner?, // null -> V5, else -> V4
        requestedSignerAccountId: AccountId?,
        options: Options
    ): Sequence<ExtrinsicBuilderFactory.Submittable> {
        val runtime = chainRegistry.getRuntime(chain.id)

        val nonceSequence = determineNonceSequence(chain, requestedSignerAccountId, options)
        val extrinsicVersion = determineExtrinsicVersion(chain, options, signer != null)
        val runtimeVersion = determineRuntimeVersion(chain, options)
        val mortality = determineMortality(chain, options)
        val metadataHashMode = determineMetadataHash(options)

        val tip = determineTip(options)

        return nonceSequence.map { nonce ->
            ExtrinsicBuilder(
                tip = tip.value,
                runtime = runtime,
                nonce = nonce,
                runtimeVersion = runtimeVersion,
                genesisHash = chain.genesisHash.value,
                blockHash = mortality.blockHash.value,
                extrinsicVersion = extrinsicVersion,
                era = mortality.era,
                signer = signer,
                accountId = requestedSignerAccountId?.value,
                batchMode = options.batchMode,
                checkMetadataHash = metadataHashMode
            ).apply {
                chargeAssetTxPayment(tip = tip.value)
            }.let { ExtrinsicBuilderFactory.Submittable(it, mortality) }
        }
    }

    private fun determineMetadataHash(options: Options): CheckMetadataHashMode {
        return options.metadataHash?.let(CheckMetadataHashMode::Enabled) ?: CheckMetadataHashMode.Disabled
    }

    private fun determineTip(options: Options): Balance {
        return options.tip ?: Balance.ZERO
    }

    private suspend fun determineMortality(chain: Chain, options: Options): Mortality {
        return options.mortality ?: mortalityConstructor.constructMortality(chain.id, rpcCalls)
    }

    private suspend fun determineRuntimeVersion(chain: Chain, options: Options): RuntimeVersion {
        if (options.specVersion != null && options.transactionVersion != null) {
            return RuntimeVersion(options.specVersion, options.transactionVersion)
        }

        val fetched = getRuntimeVersion(chain, rpcCalls)

        return RuntimeVersion(
            specVersion = options.specVersion ?: fetched.specVersion,
            transactionVersion = options.transactionVersion ?: fetched.transactionVersion
        )
    }

    private suspend fun determineNonceSequence(chain: Chain, requestedSignerAccountId: AccountId?, options: Options): Sequence<Nonce> {
        return when {
            options.nonce != null -> incrementingNonceSequence(options.nonce)
            requestedSignerAccountId == null -> zeroNonceSequence()
            else -> {
                val address = chain.addressOf(requestedSignerAccountId)
                val baseNonce = rpcCalls.getNonce(chain.id, address)
                incrementingNonceSequence(baseNonce)
            }
        }
    }

    private suspend fun determineExtrinsicVersion(chain: Chain, options: Options, isSigned: Boolean): ExtrinsicVersion {
        return options.extrinsicVersion
            ?: extrinsicVersionProvider.getDefaultExtrinsicVersion(chain.id, isSigned).getOrThrow()
    }

    private fun incrementingNonceSequence(baseNonce: BigInteger): Sequence<BigInteger> {
        var nonce = baseNonce
        return generateSequence { nonce++ }
    }

    private fun zeroNonceSequence(): Sequence<BigInteger> {
        return generateSequence { BigInteger.ZERO }
    }

    private suspend fun getRuntimeVersion(chain: Chain, rpcCalls: RpcCalls): RuntimeVersion {
        return chainDao.runtimeInfo(chain.id)?.toRuntimeVersion() ?: rpcCalls.getRuntimeVersion(chain.id)
    }
}
