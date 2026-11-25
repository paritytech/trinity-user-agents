package io.parity.truapi

import kotlinx.coroutines.runBlocking
import uniffi.truapi_provider.ChainConnection
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainProvider
import uniffi.truapi_provider.ChainProviderException
import uniffi.truapi_provider.StorageClient
import java.util.concurrent.Callable
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors

/**
 * The app's embedded light client for the chains in [chains], lower-case hex
 * genesis hashes the provider's bundled catalog knows. Construct one per
 * process: every connection to a chain shares its sync, so a second client
 * would sync the same chains again.
 *
 * smoldot stops a chain when its last connection closes. The next [connect]
 * resumes it from [storage], and [saveDatabases] keeps what is stored current.
 * With no [chains] the client serves nothing and smoldot never starts.
 */
class LightClient(
    private val storage: StorageClient,
    private val chains: Set<String>,
) {
    private val provider by lazy { ChainProvider.withStorage(storage) }

    private val started = ConcurrentHashMap.newKeySet<String>()

    fun serves(genesisHash: ByteArray): Boolean = genesisHash.toHexString() in chains

    fun servesAny(): Boolean = chains.isNotEmpty()

    /**
     * Connects to a chain this client [serves]. Blocks while the stored state is
     * read and the chain is added, so never call it on a thread that must stay
     * responsive. Requests sent before the chain first syncs are held, not lost.
     *
     * @throws ChainProviderException when the light client cannot add the chain.
     */
    fun connect(genesisHash: ByteArray, listener: ChainMessageListener): ChainConnection {
        require(serves(genesisHash)) { "the light client does not serve ${genesisHash.toHexString()}" }
        try {
            runBlocking { provider.loadDatabase(genesisHash) }
        } catch (unread: ChainProviderException) {
            // Stored state is an optimisation: without it the chain syncs from its checkpoint.
        }
        val connected = try {
            adder.submit(Callable { provider.connect(genesisHash, listener) }).get()
        } catch (failed: ExecutionException) {
            throw failed.cause ?: failed
        }
        started.add(genesisHash.toHexString())
        return connected
    }

    /** Stores the finalized state of every chain connected so far, skipping any that fail. */
    suspend fun saveDatabases() {
        started.forEach { genesis ->
            try {
                provider.saveDatabase(genesis.hexToByteArray())
            } catch (unsaved: ChainProviderException) {
                // The next save retries; a missed one only costs a colder start.
            }
        }
    }

    /**
     * smoldot's metrics for each chain connected so far that is still running, by hex genesis hash:
     * the JSON it answers `sudo_unstable_metrics` with, unchanged. A chain that stopped, or does not
     * answer, is left out.
     */
    suspend fun metrics(): Map<String, String> = buildMap {
        started.forEach { genesis ->
            try {
                provider.metrics(genesis.hexToByteArray())?.let { put(genesis, it) }
            } catch (unanswered: ChainProviderException) {
                // Metrics are for diagnosis; a chain that does not answer is simply not reported.
            }
        }
    }

    private companion object {
        // smoldot adds a chain on the calling thread and overflows the 1 MiB stack a JVM thread gets
        // by default, which kills the process. Adding chains here gives them a main thread's stack.
        const val ADD_CHAIN_STACK_BYTES = 8L shl 20

        val adder = Executors.newCachedThreadPool { task ->
            Thread(null, task, "light-client-add", ADD_CHAIN_STACK_BYTES).apply { isDaemon = true }
        }
    }
}
