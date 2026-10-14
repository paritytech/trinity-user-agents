package io.paritytech.polkadotapp.app.root.domain.main

import android.adservices.appsetid.AppSetIdManager
import android.content.Context
import android.os.Build
import android.os.ext.SdkExtensions
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.root.data.storage.OneShotTooltip
import io.paritytech.polkadotapp.app.root.data.storage.TooltipStorage
import io.paritytech.polkadotapp.app.root.domain.ObserveTabWarningsUseCase
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class MainInteractor @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val messageSender: ChatMessageSender,
    private val observeTabWarningsUseCase: ObserveTabWarningsUseCase,
    private val tooltipStorage: TooltipStorage
) {
    context(scope: ComputationalScope)
    fun initialize() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
            SdkExtensions.getExtensionVersion(SdkExtensions.AD_SERVICES) >= 6
        ) {
            AppSetIdManager.get(context)
        }

        messageSender.startExtensions()
    }

    fun observeTabWarnings(): Flow<Map<BottomTab, Boolean>> = observeTabWarningsUseCase()

    fun shouldShowScannerTooltip(): Boolean = !tooltipStorage.wasShown(OneShotTooltip.Scanner)

    fun markScannerTooltipShown() {
        tooltipStorage.markShown(OneShotTooltip.Scanner)
    }
}
