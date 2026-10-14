package io.paritytech.polkadotapp.feature_chats_impl.domain.devices

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.OurDevicesProvider
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import javax.inject.Inject

class RealOurDevicesProvider @Inject constructor(
    private val accountRepository: AccountRepository,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val getActiveSsoSessions: GetActiveSsoSessionsUseCase,
) : OurDevicesProvider {
    override suspend fun getOurDevices(): List<DeviceInfo> {
        val walletAccount = accountRepository.getWalletAccount()
        val mobile = DeviceInfo(
            statementAccountId = walletAccount.defaultAccountId(),
            encryptionPublicKey = ourDeviceKeypairProvider.publicKey(),
        )

        val desktops = getActiveSsoSessions.getSessions().map { session ->
            DeviceInfo(
                statementAccountId = session.statementAccountId,
                encryptionPublicKey = session.encryptionPublicKey,
            )
        }

        return listOf(mobile) + desktops
    }
}
