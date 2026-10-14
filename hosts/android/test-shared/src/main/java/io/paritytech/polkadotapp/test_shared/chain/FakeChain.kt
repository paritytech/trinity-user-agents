package io.paritytech.polkadotapp.test_shared.chain

data class FakeBlock<S>(
    val number: Long,
    val hash: String,
    val parentHash: String,
    val state: S,
    val body: List<String>,
)

/**
 * A block tree with per-block state, finalization and reorgs, generic over the state a feature tracks.
 *
 * Reorged-out blocks stay readable by hash, the way a node still serves an orphaned block; only the
 * canonical chain moves. [reorgDepths] is bounded by the unfinalized suffix, so a finalized block cannot
 * be reorged and no failure a fuzzer finds is merely a violated Requirement.
 */
class FakeChain<S>(initialState: S) {
    private val blocks = mutableMapOf<String, FakeBlock<S>>()
    private val canonical = mutableListOf<FakeBlock<S>>()
    private var nextHashSeq = 0L
    private var finalizedNumber = 0L

    init {
        val genesis = FakeBlock(number = 0, hash = nextHash(), parentHash = "", state = initialState, body = emptyList())
        blocks[genesis.hash] = genesis
        canonical += genesis
    }

    val bestHead: FakeBlock<S> get() = canonical.last()

    val finalizedHead: FakeBlock<S> get() = canonical[finalizedNumber.toInt()]

    /** Empty when nothing is unfinalized, which is what makes a finalized block unreorgable. */
    val reorgDepths: IntRange get() = 1..(bestHead.number - finalizedNumber).toInt()

    fun produceBlock(body: List<String> = emptyList(), mutate: (S) -> S = { it }): FakeBlock<S> {
        val parent = bestHead
        val block = FakeBlock(
            number = parent.number + 1,
            hash = nextHash(),
            parentHash = parent.hash,
            state = mutate(parent.state),
            body = body,
        )
        blocks[block.hash] = block
        canonical += block

        return block
    }

    fun finalize(upTo: Long) {
        require(upTo <= bestHead.number) { "Cannot finalize $upTo above best head ${bestHead.number}" }
        require(upTo >= finalizedNumber) { "Finalization is monotone: $upTo is below $finalizedNumber" }

        finalizedNumber = upTo
    }

    /** Rewinds the canonical head by [depth]; the caller produces the replacing branch. */
    fun reorg(depth: Int) {
        require(depth in reorgDepths) { "Reorg depth $depth outside $reorgDepths" }

        repeat(depth) { canonical.removeAt(canonical.lastIndex) }
    }

    fun stateAt(hash: String): S? = blocks[hash]?.state

    fun blockAt(hash: String): FakeBlock<S>? = blocks[hash]

    fun canonicalAt(number: Long): FakeBlock<S>? = canonical.getOrNull(number.toInt())

    private fun nextHash(): String = "0x" + (nextHashSeq++).toString(16).padStart(64, '0')
}
