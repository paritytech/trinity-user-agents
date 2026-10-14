package io.paritytech.polkadotapp.feature_chats_impl.domain.devices

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.forEachAsync
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.BroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.OurDevicesProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasPendingChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import timber.log.Timber
import javax.inject.Inject

class ContactDeviceFanOutService @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val ourDevicesProvider: OurDevicesProvider,
    private val broadcastDeviceLifecycleUseCase: BroadcastDeviceLifecycleUseCase,
    private val accountRepository: AccountRepository
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        combine(
            accountRepository.walletAccountFlow(),
            contactsRepository.subscribePendingFanOutContacts()
        ) { walletAccount, contacts ->
            contacts.filter { it.isMultiDeviceChatSupported(walletAccount) }
        }
            .distinctUntilChangedBy { it.changeKeys() }
            .onEach { pending ->
                pending.filterNot { it.hasPendingChatRequest() }
                    .forEachAsync { fanOutTo(it) }
            }
            .launchIn(scope)
    }

    // TODO: We can remove this from key: it.hasPendingChatRequest() after we fix TODO in RealIncomingChatRequestProcessor.createNewIncomingRequestChat
    private fun List<Contact>.changeKeys() = map { it.pendingDevicesFanOut to it.hasPendingChatRequest() }

    private suspend fun fanOutTo(contact: Contact) {
        val ourDevices = ourDevicesProvider.getOurDevices()

        Timber.d("Fan out to contact: " + contact.accountId.value.toHexString())

        for (device in ourDevices) {
            Timber.d("Fan out to contact: device: " + device.statementAccountId.value.toHexString())

            broadcastDeviceLifecycleUseCase.sendDeviceAddedTo(
                contactAccountId = contact.accountId,
                statementAccountId = device.statementAccountId,
                encryptionPublicKey = device.encryptionPublicKey,
            ).logFailure("Fan-out DeviceAdded(${device.statementAccountId}) to ${contact.accountId} failed")
        }

        contactsRepository.markDevicesFannedOut(contact.accountId)
    }
}
