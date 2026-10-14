package io.paritytech.polkadotapp.chains.util

import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.TimerValue
import io.paritytech.polkadotapp.common.utils.toTimerValue
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

interface BlockDurationEstimator {
    val currentBlock: BlockNumber

    fun durationUntil(block: BlockNumber): Duration

    fun durationOf(numberOfBlocks: BlockNumber): Duration

    fun timestampAt(block: BlockNumber): Timestamp

    fun blockInFuture(duration: Duration): BlockNumber
}

fun BlockDurationEstimator.blockInPast(duration: Duration): BlockNumber {
    return blockInFuture(-duration)
}

fun BlockDurationEstimator.timerUntil(block: BlockNumber): TimerValue {
    return durationUntil(block).toTimerValue()
}

fun BlockDurationEstimator.timerOf(block: BlockNumber): TimerValue {
    return durationOf(block).toTimerValue()
}

fun BlockDurationEstimator.timestampAfter(blocks: BlockNumber): Timestamp {
    return timestampAt(currentBlock + blocks)
}

fun BlockDurationEstimator(currentBlock: BlockNumber, blockTimeMillis: BigInteger): BlockDurationEstimator {
    return RealBlockDurationEstimator(currentBlock, blockTimeMillis)
}

internal class RealBlockDurationEstimator(
    override val currentBlock: BlockNumber,
    private val blockTimeMillis: BigInteger
) : BlockDurationEstimator {
    override fun durationUntil(block: BlockNumber): Duration {
        val blocksInFuture = block - currentBlock

        return durationOf(blocksInFuture)
    }

    override fun durationOf(blocks: BlockNumber): Duration {
        if (blocks < BlockNumber.ZERO) return Duration.ZERO

        val millisInFuture = blocks.value * blockTimeMillis

        return millisInFuture.toLong().milliseconds
    }

    override fun timestampAt(block: BlockNumber): Long {
        val offsetInBlocks = block - currentBlock
        val offsetInMillis = offsetInBlocks.value * blockTimeMillis

        val currentTime = System.currentTimeMillis()

        return currentTime + offsetInMillis.toLong()
    }

    override fun blockInFuture(duration: Duration): BlockNumber {
        val offsetInBlocks = duration.inWholeMilliseconds.toBigInteger() / blockTimeMillis

        return currentBlock + offsetInBlocks.toBlockNumber()
    }
}
