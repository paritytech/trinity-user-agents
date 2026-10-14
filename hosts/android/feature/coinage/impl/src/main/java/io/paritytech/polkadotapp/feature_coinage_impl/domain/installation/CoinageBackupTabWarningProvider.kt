package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.presentation.tabs.TabWarningProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.needsAttention
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class CoinageBackupTabWarningProvider @Inject constructor(
    private val backupObserver: CoinageAccountBackupObserver,
) : TabWarningProvider {
    override val tab: BottomTab = BottomTab.WALLET

    override fun observeWarning(): Flow<Boolean> = backupObserver.subscribeStatus().map { it.needsAttention }
}
