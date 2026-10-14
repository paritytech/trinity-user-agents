package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.mockito.Mockito.mock

/** A provider that answers [age], for the strategies that read the chain limit when they decide. */
fun forcedAgeOf(age: Int) = ForcedRecyclingAgeProvider(
    mock<CoinRepository>().apply {
        runBlocking { whenever(getCoinRecyclingAge()).thenReturn(Result.success(age)) }
    }
)
