package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.HopTransferResumeInitializer
import io.paritytech.polkadotapp.feature_chats_impl.data.worker.ChatRequestRenewalScheduler
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryService
import io.paritytech.polkadotapp.feature_chats_impl.domain.devices.ContactDeviceFanOutService
import javax.inject.Inject

/** Starts the native person chat's services, unless this launch runs the chat product (HX2). */
internal class NativePersonChatServices @Inject constructor(
    private val mode: PersonChatMode,
    private val renewal: ChatRequestRenewalScheduler,
    fanOut: ContactDeviceFanOutService,
    delivery: ChatRequestDeliveryService,
    chainBridge: ContactSessionChainBridge,
    lifecycle: ContactSessionLifecycleBinder,
    sessions: RealContactChatSessionManagerInitializer,
    hopResume: HopTransferResumeInitializer,
) : AppInitializer {
    private val services = listOf(fanOut, delivery, renewal, chainBridge, lifecycle, sessions, hopResume)

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        if (mode.isProductChat()) return renewal.cancel()
        return services.map { it.initialize() }.firstOrNull { it.isFailure } ?: Result.success(Unit)
    }
}
