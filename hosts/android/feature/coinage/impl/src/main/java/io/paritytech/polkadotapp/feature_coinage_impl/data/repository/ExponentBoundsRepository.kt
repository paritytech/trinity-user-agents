package io.paritytech.polkadotapp.feature_coinage_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.maxExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.minExponent
import javax.inject.Inject

interface ExponentBoundsRepository {
    suspend fun minExponent(chainId: ChainId): Result<Int>
    suspend fun maxExponent(chainId: ChainId): Result<Int>
}

suspend fun ExponentBoundsRepository.validateValueExponent(chainId: ChainId, valueExponent: ValueExponent): Result<ValueExponent> =
    getAllowedExponents(chainId).flatMap { allowed ->
        if (valueExponent in allowed) Result.success(valueExponent)
        else Result.failure(IllegalArgumentException("Invalid exponent ${valueExponent.value}. Allowed: $allowed"))
    }

suspend fun ExponentBoundsRepository.validateValueExponents(chainId: ChainId, valueExponents: List<ValueExponent>): Result<List<ValueExponent>> =
    getAllowedExponents(chainId).flatMap { allowed ->
        val allExponentsAllowed = valueExponents.all { it in allowed }
        if (allExponentsAllowed) Result.success(valueExponents)
        else Result.failure(IllegalArgumentException("Invalid exponent in list: ${valueExponents.map { it.value }.joinToString()}. Allowed: ${allowed.map { it.value }.joinToString()}"))
    }

suspend fun ExponentBoundsRepository.getAllowedExponents(chainId: ChainId): Result<Set<ValueExponent>> =
    minExponent(chainId)
        .flatMap { min ->
            maxExponent(chainId)
                .map { max ->
                    (min..max)
                        .mapToSet { ValueExponent(it) }
                }
        }

class RealExponentBoundsRepository @Inject constructor(
    private val chainRegistry: ChainRegistry
) : ExponentBoundsRepository {
    override suspend fun maxExponent(chainId: ChainId): Result<Int> {
        return runCatching {
            chainRegistry.withRuntime(chainId) {
                runtime.metadata.coinage.maxExponent
            }
        }
    }

    override suspend fun minExponent(chainId: ChainId): Result<Int> {
        return runCatching {
            chainRegistry.withRuntime(chainId) {
                runtime.metadata.coinage.minExponent
            }
        }
    }
}
