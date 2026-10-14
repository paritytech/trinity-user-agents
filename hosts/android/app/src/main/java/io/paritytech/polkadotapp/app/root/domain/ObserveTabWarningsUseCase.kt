package io.paritytech.polkadotapp.app.root.domain

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.presentation.tabs.TabWarningProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

class ObserveTabWarningsUseCase @Inject constructor(
    private val warningProviders: Set<@JvmSuppressWildcards TabWarningProvider>
) {
    operator fun invoke(): Flow<Map<BottomTab, Boolean>> {
        if (warningProviders.isEmpty()) {
            return flowOf(emptyMap())
        }

        val flows = warningProviders.map { provider ->
            provider.observeWarning()
        }

        return combine(flows) { warnings ->
            warningProviders.zip(warnings.toList()) { provider, hasWarning ->
                provider.tab to hasWarning
            }
                .groupBy({ it.first }, { it.second })
                .mapValues { (_, values) -> values.any { it } }
        }
    }
}
