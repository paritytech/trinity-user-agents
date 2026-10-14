package io.paritytech.polkadotapp.feature_sso_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_sso_impl.SsoRouter
import io.paritytech.polkadotapp.feature_sso_impl.data.SsoHandshakeProtocol
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.toPayload
import javax.inject.Inject

internal class PairDeepLinkHandler @Inject constructor(
    private val accountRepository: AccountRepository,
    private val ssoHandshakeProtocol: SsoHandshakeProtocol,
    private val ssoRouter: SsoRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        return FeatureOption.LINKED_DEVICES.isEnabled && ssoHandshakeProtocol.isPairingDeeplink(data)
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> {
        accountRepository.awaitAccountsInitialized()

        return ssoHandshakeProtocol.parsePairDeeplink(data)
            .map { offer ->
                DeeplinkProcessingOutcome.Navigate {
                    ssoRouter.openPairRequest(offer.toPayload())
                }
            }
    }
}
