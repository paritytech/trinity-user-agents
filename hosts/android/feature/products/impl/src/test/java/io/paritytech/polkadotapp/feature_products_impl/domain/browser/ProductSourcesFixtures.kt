package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.mockk.every
import io.mockk.mockk

fun productSources(devServersEnabled: Boolean = true) = ProductSources(
    mockk<DevServers> { every { enabled } returns devServersEnabled },
    mockk { every { currentTldOrNull() } returns null },
    mockk(),
)
