package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.interceptor.WebSocketResponseInterceptor
import io.novasama.substrate_sdk_android.wsrpc.interceptor.WebSocketResponseInterceptor.ResponseDelivery
import io.novasama.substrate_sdk_android.wsrpc.networkStateFlow
import io.novasama.substrate_sdk_android.wsrpc.response.RpcResponse
import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.connection.autobalance.NodeAutobalancer
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Provider
import javax.inject.Singleton
import kotlin.random.Random

@Singleton
class ChainConnectionFactory @Inject constructor(
    private val refCounter: ChainConnectionRefCounter,
    private val nodeAutobalancer: NodeAutobalancer,
    private val socketServiceProvider: Provider<SocketService>,
    private val networkStateService: NetworkStateService,
    private val shortLivedConnectionDetector: ShortLivedConnectionDetector,
) {
    suspend fun create(chain: Chain): ChainConnection {
        val connection = ChainConnection(
            socketService = socketServiceProvider.get(),
            refCounter = refCounter,
            nodeAutobalancer = nodeAutobalancer,
            chain = chain,
            networkStateService = networkStateService,
            shortLivedConnectionDetector = shortLivedConnectionDetector,
        )

        connection.setup()

        return connection
    }
}

private const val INFURA_ERROR_CODE = -32005
private const val ALCHEMY_ERROR_CODE = 429

private const val BLUST_CAPACITY_ERROR_CODE = -32098
private const val BLUST_RATE_LIMIT_ERROR_CODE = -32097

private val RATE_LIMIT_ERROR_CODES =
    listOf(
        INFURA_ERROR_CODE,
        ALCHEMY_ERROR_CODE,
        BLUST_CAPACITY_ERROR_CODE,
        BLUST_RATE_LIMIT_ERROR_CODE
    )

private const val NODE_SWITCH_ATTEMPT_THRESHOLD = 3

private const val RECONNECT_JITTER_MAX_MILLIS = 750L

private const val NETWORK_RECONNECT_DEBOUNCE_MILLIS = 1_000L

class ChainConnection internal constructor(
    val socketService: SocketService,
    private val refCounter: ChainConnectionRefCounter,
    nodeAutobalancer: NodeAutobalancer,
    private val chain: Chain,
    private val networkStateService: NetworkStateService,
    shortLivedConnectionDetector: ShortLivedConnectionDetector,
) : CoroutineScope by CoroutineScope(Dispatchers.Default),
    WebSocketResponseInterceptor {
    val state = socketService.networkStateFlow()
        .stateIn(scope = this, started = SharingStarted.Eagerly, initialValue = State.Disconnected)

    private val responseRequiresNodeChangeFlow = MutableSharedFlow<Unit>(replay = 0, extraBufferCapacity = 1)

    private val nodeChangeSignal = merge(
        state.nodeChangeEvents(),
        shortLivedConnectionDetector.nodeChangeEvents(state),
        responseRequiresNodeChangeFlow
    ).shareIn(scope = this, started = SharingStarted.Eagerly)

    private val availableNodes = MutableStateFlow(chain.nodes)

    private val currentUrl =
        nodeAutobalancer.connectionUrlFlow(
            chainId = chain.id,
            changeConnectionEventFlow = nodeChangeSignal,
            availableNodesFlow = availableNodes,
        )
            .shareIn(scope = this, started = SharingStarted.Eagerly, replay = 1)

    internal suspend fun setup() {
        socketService.setInterceptor(this)

        observeCurrentNode()

        launch {
            refCounter.shouldConnectionBeEnabled(chain.id)
                .distinctUntilChanged()
                .collectLatest { shouldBeEnabled ->
                    if (shouldBeEnabled) {
                        delay(Random.nextLong(RECONNECT_JITTER_MAX_MILLIS))
                        socketService.resume()
                    } else {
                        socketService.pause()
                    }
                }
        }

        observeNetworkForReconnect()
    }

    private fun observeNetworkForReconnect() {
        networkStateService.isNetworkAvailable
            .debounce(NETWORK_RECONNECT_DEBOUNCE_MILLIS)
            .drop(1)
            .filter { isAvailable -> isAvailable }
            .onEach { (state.value as? State.WaitingForReconnect)?.let { socketService.switchUrl(it.url) } }
            .launchIn(this)
    }

    private suspend fun observeCurrentNode() {
        val firstNodeUrl = currentUrl.first()?.saturatedUrl ?: return
        socketService.start(firstNodeUrl, remainPaused = true)

        currentUrl
            .mapNotNull { it?.saturatedUrl }
            .filter { nodeUrl -> actualUrl() != nodeUrl }
            .onEach { nodeUrl -> socketService.switchUrl(nodeUrl) }
            .onEach { nodeUrl -> Timber.d("Switching node in ${chain.name} to $nodeUrl") }
            .launchIn(this)
    }

    fun considerUpdateNodes(nodes: Chain.Nodes) {
        availableNodes.value = nodes
    }

    fun finish() {
        cancel()

        socketService.stop()
    }

    private suspend fun actualUrl(): String? {
        return when (val stateSnapshot = state.first()) {
            is State.WaitingForReconnect -> stateSnapshot.url
            is State.Connecting -> stateSnapshot.url
            is State.Connected -> stateSnapshot.url
            State.Disconnected -> null
            is State.Paused -> stateSnapshot.url
        }
    }

    private fun Flow<State>.nodeChangeEvents(): Flow<Unit> {
        return mapNotNull { stateValue ->
            Unit.takeIf { stateValue.needsAutobalance() }
        }
    }

    /**
     * Trigger a node switch only after the SDK has retried the current node a few times.
     *
     * [SocketService.switchUrl] resets the SDK's reconnect attempt counter to 0, which pins the
     * SDK's exponential backoff (300ms base, base-2) at its floor: if we rotate nodes on every
     * reconnect the backoff never escalates and the socket fast-fail-loops. Switching only after
     * [NODE_SWITCH_ATTEMPT_THRESHOLD] attempts lets the backoff grow a few steps on the same node
     * before we rotate, so a flaky network produces far fewer reconnect attempts.
     */
    private fun State.needsAutobalance() =
        this is State.WaitingForReconnect && attempt >= NODE_SWITCH_ATTEMPT_THRESHOLD

    override fun onRpcResponseReceived(rpcResponse: RpcResponse): ResponseDelivery {
        val error = rpcResponse.error

        return if (error != null && error.code in RATE_LIMIT_ERROR_CODES) {
            Timber.d("Received rate limit exceeded error code in rpc response. Switching to another node")

            responseRequiresNodeChangeFlow.tryEmit(Unit)

            ResponseDelivery.DROP
        } else {
            ResponseDelivery.DELIVER_TO_SENDER
        }
    }
}
