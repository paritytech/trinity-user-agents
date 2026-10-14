package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository

/** A provider that answers [age], for the strategies that read the chain limit when they decide. */
fun forcedAgeOf(age: Int) = ForcedRecyclingAgeProvider(
    mockk<CoinRepository> {
        coEvery { getCoinRecyclingAge() } returns Result.success(age)
    }
)
