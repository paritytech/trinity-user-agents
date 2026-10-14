package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.presentation.loading.map
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_fund_api.domain.model.AutoConvertDeposit.Status
import io.paritytech.polkadotapp.feature_fund_api.domain.model.DepositId
import io.paritytech.polkadotapp.feature_fund_impl.FundRouter
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.FundInteractor
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.model.depositAddress
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.ConversionModel
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundUiState
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.FiatAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.domain.AssetDisplayMapper
import io.paritytech.polkadotapp.feature_tokens_api.domain.requireDisplayOf
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetPayload
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.toFullChainAssetId
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

@HiltViewModel
class FundViewModel @Inject constructor(
    private val router: FundRouter,
    savedStateHandle: SavedStateHandle,
    private val mapper: AssetDisplayMapper,
    private val interactor: FundInteractor,
    private val clipboardService: ClipboardService,
    private val tokenAmountMapper: TokenAmountMapper,
    private val fiatMapper: FiatAmountMapper,
) : BaseViewModel(), FundContract {
    private val operations = mutableMapOf<DepositId, FundingOperation>()

    private val payload: AssetPayload = savedStateHandle.getPayload()

    private val currentDeposit = interactor.currentDeposit()

    private val depositCredentials = flowOf {
        interactor.fundingCredentials(payload.toFullChainAssetId())
    }

    private val depositTerms = flowOf {
        interactor.depositTerms(payload.toFullChainAssetId())
    }
        .withLoading()

    override val state =
        combine(
            depositCredentials,
            depositTerms,
            currentDeposit
        ) { depositCredentials, depositTermsLoading, currentDeposit ->
            depositTermsLoading.map { depositTerms ->
                val asset = depositCredentials.chainWithAsset.asset
                val conversion = depositTerms.conversionRate

                currentDeposit?.let { deposit ->
                    val from = tokenAmountMapper.mapFrom(deposit.amount)

                    val toAmount = when (val status = deposit.status) {
                        is Status.InProgress -> status.expectedAmount
                        is Status.Failure -> asset.withAmount(deposit.amount.amount * conversion.rate())
                        is Status.SwapCompleted -> status.actualConvertedAmount
                        is Status.Done -> status.actualAmount
                    }

                    val to = tokenAmountMapper.mapFrom(toAmount)

                    operations[deposit.id] = FundingOperation(
                        id = deposit.id.toString(),
                        status = deposit.status.toUi(),
                        conversion = from to to,
                    )
                }

                FundUiState(
                    doneEnabled = true,
                    assetDisplay = mapper.requireDisplayOf(asset),
                    chainName = depositCredentials.chainWithAsset.chain.name,
                    fundingAddress = depositCredentials.depositAddress(),
                    minimumSendAmount = tokenAmountMapper.mapFrom(asset.withAmount(depositTerms.minDeposit)),
                    fee = fiatMapper.mapToUi(depositTerms.estimatedFee),
                    conversion = ConversionModel(
                        from = tokenAmountMapper.mapFrom(conversion.sampleFrom()),
                        to = tokenAmountMapper.mapFrom(conversion.sampleTo())
                    ),
                    operations = operations.values.toImmutableList()
                )
            }
        }
            .stateIn(
                scope = this,
                started = SharingStarted.Eagerly,
                initialValue = LoadingState.Loading
            )

    override fun doneClicked() {
        val isInProgress = state.value.dataOrNull?.operations?.any {
            it.status is FundingOperation.Status.InProgress
        } == true

        if (isInProgress) {
            router.openConfirmationScreen()
        } else {
            router.back()
        }
    }

    override fun copyAddressClicked(address: String) {
        clipboardService.setPrimaryClip(address)
    }

    fun Status.toUi(): FundingOperation.Status {
        return when (this) {
            is Status.InProgress -> FundingOperation.Status.InProgress((completesAt - System.currentTimeMillis()).milliseconds)
            is Status.SwapCompleted -> FundingOperation.Status.InProgress(Duration.ZERO)
            is Status.Failure -> FundingOperation.Status.Failure
            is Status.Done -> FundingOperation.Status.Done
        }
    }
}
