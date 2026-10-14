package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.annotations.JsonAdapter
import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.ProductAccountIdTupleAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import kotlinx.coroutines.flow.map

class StatementHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerSubscription<StatementStoreSubscribeParams, StatementsPageDto>("statementStoreSubscribe") { params ->
            botApi.subscribeStatements(params.filter.toDomain()).map { page ->
                StatementsPageDto(
                    statements = page.statements.map { it.toDto() },
                    isComplete = page.isComplete,
                )
            }
        }

        bridge.registerHandler<CreateProofForAccountParams, StatementProofDto>("createStatementProof") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val body = params.statement.toStatementBody()
            botApi.createStatementProof(productId, params.account, body).map { proof ->
                StatementProofDto(
                    tag = "Sr25519",
                    signature = proof.signature.signature.toHexString(withPrefix = true),
                    signer = proof.publicKey.toHexString(withPrefix = true),
                )
            }
        }

        bridge.registerHandler<CreateStatementProofParams, StatementProofDto>("createStatementProofAuthorized") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val body = params.toStatementBody()
            botApi.createStatementProofAuthorized(productId, body).map { proof ->
                StatementProofDto(
                    tag = "Sr25519",
                    signature = proof.signature.signature.toHexString(withPrefix = true),
                    signer = proof.publicKey.toHexString(withPrefix = true),
                )
            }
        }

        bridge.registerHandler<StatementSubmitParams, Unit>("statementStoreSubmit") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            botApi.submitStatement(productId, params.toStatement())
        }
    }
}

private data class StatementStoreSubscribeParams(val filter: TopicFilterDto)

private data class TopicFilterDto(
    val matchAll: List<HexString>? = null,
    val matchAny: List<HexString>? = null,
)

private fun TopicFilterDto.toDomain(): TopicFilter = when {
    matchAll != null -> TopicFilter.MatchAll(matchAll.map { it.fromHex() })
    matchAny != null -> TopicFilter.MatchAny(matchAny.map { it.fromHex() })
    else -> throw IllegalArgumentException("TopicFilter must specify matchAll or matchAny")
}

private data class StatementsPageDto(
    val statements: List<StatementDto>,
    val isComplete: Boolean,
)

private data class StatementDto(
    val proof: StatementProofDto,
    val channel: HexString?,
    val expiry: String,
    val topics: List<HexString>,
    val data: HexString?,
)

private data class StatementProofDto(
    val tag: String,
    val signature: HexString,
    val signer: HexString,
)

private data class CreateProofForAccountParams(
    @JsonAdapter(ProductAccountIdTupleAdapter::class)
    val account: ProductAccountId,
    val statement: CreateStatementProofParams,
)

private data class CreateStatementProofParams(
    val channel: HexString?,
    val expiry: String?,
    val topics: List<HexString>,
    val data: HexString?,
) {
    fun toStatementBody() = Statement.Body(
        channel = channel?.fromHex(),
        expiry = expiry?.toULong() ?: StatementExpiry.createDefault(),
        topic1 = topics.getOrNull(0)?.fromHex(),
        topic2 = topics.getOrNull(1)?.fromHex(),
        topic3 = topics.getOrNull(2)?.fromHex(),
        topic4 = topics.getOrNull(3)?.fromHex(),
        data = data?.fromHex() ?: ByteArray(0),
    )
}

private data class StatementSubmitParams(
    val proof: StatementProofDto,
    val channel: HexString?,
    val expiry: String?,
    val topics: List<HexString>,
    val data: HexString?,
) {
    fun toStatement() = Statement(
        proof = StatementStoreMessageProof(
            signature = SignatureWrapper.Sr25519(proof.signature.fromHex()),
            publicKey = proof.signer.fromHex(),
        ),
        body = Statement.Body(
            channel = channel?.fromHex(),
            expiry = expiry?.toULong() ?: StatementExpiry.createDefault(),
            topic1 = topics.getOrNull(0)?.fromHex(),
            topic2 = topics.getOrNull(1)?.fromHex(),
            topic3 = topics.getOrNull(2)?.fromHex(),
            topic4 = topics.getOrNull(3)?.fromHex(),
            data = data?.fromHex() ?: ByteArray(0),
        )
    )
}

private fun Statement.toDto() = StatementDto(
    proof = StatementProofDto(
        tag = "Sr25519",
        signature = proof.signature.signature.toHexString(withPrefix = true),
        signer = proof.publicKey.toHexString(withPrefix = true),
    ),
    channel = body.channel?.toHexString(withPrefix = true),
    expiry = body.expiry.toString(),
    topics = listOfNotNull(body.topic1, body.topic2, body.topic3, body.topic4)
        .map { it.toHexString(withPrefix = true) },
    data = body.data.toHexString(withPrefix = true),
)
