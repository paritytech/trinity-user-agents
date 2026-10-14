package io.paritytech.polkadotapp.feature_statement_store_impl.data

import com.google.gson.Gson
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.request.DeliveryType
import io.novasama.substrate_sdk_android.wsrpc.subscription.response.SubscriptionChange
import io.novasama.substrate_sdk_android.wsrpc.subscriptionFlow
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.getSocket
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementsPage
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.requests.SubmitStatementRequest
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.requests.SubscribeStatementRequest
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response.StatementSubscriptionResponse
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response.SubmitStatementOutcome
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response.SubmitStatementResult
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response.determineOutcome
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.toDomain
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.toRemote
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.StatementRemote as ScaleStatement

private val RETRY_DELAY = 2.seconds
private const val MAX_RETRIES = 10

class RealStatementStoreService @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val peer: RealStatementStorePeer,
    private val gson: Gson
) : StatementStoreService {
    override suspend fun submitStatement(statement: Statement): Result<Unit> = runCatching {
        val request = statement.toRequest()

        var lastResult: SubmitStatementResult? = null

        repeat(MAX_RETRIES) { attempt ->
            val submitResult = attemptSubmit(request)

            when (submitResult.determineOutcome()) {
                SubmitStatementOutcome.ACCEPTED -> return@runCatching
                SubmitStatementOutcome.RETRIABLE_FAILURE -> {
                    lastResult = submitResult

                    Timber.e("Attempt ${attempt + 1} of $MAX_RETRIES failed: $submitResult. Retrying in $RETRY_DELAY...")

                    delay(RETRY_DELAY)
                }
                SubmitStatementOutcome.FATAL_FAILURE -> {
                    error("Fatal statement store submission error: $submitResult")
                }
            }
        }

        error("Statement submit failed after $MAX_RETRIES attempts! Last result: $lastResult")
    }

    override suspend fun submitStatementOnce(statement: Statement): Result<Unit> = runCatching {
        val submitResult = attemptSubmit(statement.toRequest())

        when (submitResult.determineOutcome()) {
            SubmitStatementOutcome.ACCEPTED -> Unit
            else -> error("Single-attempt statement submit failed: $submitResult")
        }
    }

    private fun Statement.toRequest(): SubmitStatementRequest {
        val remote = toRemote()
        val encoded = BinaryScale.encodeToByteArray(remote).toHexString()
        return SubmitStatementRequest(encoded)
    }

    private suspend fun attemptSubmit(request: SubmitStatementRequest): SubmitStatementResult {
        val response = getSocketService().executeAsync(
            request = request,
            deliveryType = DeliveryType.AT_MOST_ONCE
        )
        return SubmitStatementResult.parse(gson, response.result)
    }

    override suspend fun fetchStatements(filter: TopicFilter): Result<List<Statement>> = runCatching {
        val socketService = getSocketService()
        val allStatements = mutableListOf<Statement>()

        socketService.subscriptionFlow(
            request = SubscribeStatementRequest(filter),
            unsubscribeMethod = "statement_unsubscribeStatement"
        ).transformWhile { change ->
            val response = parseSubscriptionChange(change)
            val statements = response.data.statements.map { parseStatement(it) }
            emit(statements)

            response.data.remaining > 0
        }.collect { statements ->
            allStatements.addAll(statements)
        }

        allStatements
    }

    override fun subscribeStatements(filter: TopicFilter): Flow<Result<StatementsPage>> = peer.track(
        flow {
            val socketService = getSocketService()

            emitAll(
                socketService.subscriptionFlow(
                    request = SubscribeStatementRequest(filter),
                    unsubscribeMethod = "statement_unsubscribeStatement"
                ).map { change ->
                    runCatching {
                        val response = parseSubscriptionChange(change)
                        val statements = response.data.statements.map { parseStatement(it) }
                        StatementsPage(statements = statements, isComplete = response.data.remaining == 0)
                    }
                }
            )
        }
    )

    private fun parseSubscriptionChange(change: SubscriptionChange): StatementSubscriptionResponse {
        val tree = gson.toJsonTree(change.params.result)
        return gson.fromJson(tree, StatementSubscriptionResponse::class.java)
    }

    private fun parseStatement(hexContent: String): Statement {
        val remote = BinaryScale.decodeFromByteArray<ScaleStatement>(hexContent.fromHex())
        return remote.toDomain()
    }

    private suspend fun getSocketService(): SocketService {
        return chainRegistry.getSocket(peer.chainId)
    }
}
