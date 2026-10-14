package io.paritytech.polkadotapp.feature_wallet_impl.deeplink

import android.net.Uri
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.toAccountId
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.getQueryParameterOrThrow
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.toParcel
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.AmountPreset
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class PayDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val pocketRouter: PocketRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        // The address-unload pay deeplink is host-only (`polkadotapp://pay?address=..`); sub-paths under
        // the `pay` host belong to other native payment flows (e.g. `pay/cheque` -> W3S coinage cheque).
        return data.scheme == DeepLinkHandler.APP_SCHEME && data.host == PAY_HOST && data.path.isNullOrEmpty()
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.io) {
        runCancellableCatching {
            accountRepository.awaitAccountsInitialized()

            val addressString = data.getQueryParameterOrThrow(PARAM_ADDRESS)
            val amountPlanks = data.getQueryParameterOrThrow(PARAM_AMOUNT).toBigInteger()
            val lockAmount = data.getQueryParameter(PARAM_LOCK_AMOUNT)?.toBooleanStrictOrNull() ?: true

            val accountId = addressString.toAccountId().intoAccountId()

            val extractedAddress = ExtractedAddress(
                display = addressString,
                type = ExtractedAddress.DisplayType.ADDRESS,
                accountId = accountId,
            )

            val payload = SendEnterAmountPayload(
                showTransactionResult = true,
                transferMethod = TransferMethodPayload.UnloadIntoExternal(extractedAddress.toParcel()),
                amountPreset = AmountPreset(amount = amountPlanks, lockAmount = lockAmount),
            )

            DeeplinkProcessingOutcome.Navigate {
                pocketRouter.openSendEnterAmountFromDeeplink(payload)
            }
        }
    }

    companion object {
        const val PAY_HOST = "pay"
        const val PARAM_ADDRESS = "address"
        const val PARAM_AMOUNT = "amount"
        const val PARAM_LOCK_AMOUNT = "lockAmount"
    }
}
