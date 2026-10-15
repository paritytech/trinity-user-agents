package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchPhase
import io.paritytech.polkadotapp.feature_account_api.presentation.address.converter.ParseAddressConverterFactory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.AddressCandidates
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesCategory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_api.presentation.ChatStarter
import io.paritytech.polkadotapp.feature_chats_api.presentation.address.ContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transfers_api.presentation.PreviousPaymentsAddressConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.ParseAddressUsernameConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.UsernameAddressConverterFactory
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.OFFRAMP_URL
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.ONRAMP_URL
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.ParkedFundingDomainProvider
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.WithdrawUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain.RealSendPaymentInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetNeverOpened
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetOpenedOnce
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetOpenedTwice
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.assertPresentationErrorShown
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

private const val CHAIN_ID = "chain"
private val STATE_TIMEOUT = 1.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class SendPaymentViewModelTest {
    private val router: PocketRouter = mock(PocketRouter::class.java)
    private val clipboardService: ClipboardService = mock(ClipboardService::class.java)
    private val getContactsUseCase: GetContactsUseCase = mock(GetContactsUseCase::class.java)
    private val chatStarter: ChatStarter = mock(ChatStarter::class.java)
    private val addressInputMixinFactory: AddressInputMixin.Factory = mock(AddressInputMixin.Factory::class.java)
    private val addressInputMixin: AddressInputMixin = mock(AddressInputMixin::class.java)
    private val usernameAddressConverterFactory: UsernameAddressConverterFactory =
        mock(UsernameAddressConverterFactory::class.java)
    private val parseAddressConverterFactory: ParseAddressConverterFactory = mock(ParseAddressConverterFactory::class.java)
    private val parseAddressUsernameConverterFactory: ParseAddressUsernameConverterFactory =
        mock(ParseAddressUsernameConverterFactory::class.java)
    private val previousPaymentsAddressConverterFactory: PreviousPaymentsAddressConverterFactory =
        mock(PreviousPaymentsAddressConverterFactory::class.java)
    private val contactsAddressConverterFactory: ContactsAddressConverterFactory =
        mock(ContactsAddressConverterFactory::class.java)
    private val chainAssetProvider: ChainAssetProvider = mock(ChainAssetProvider::class.java)
    private val addressConverter: AddressInputMixin.AddressConverter = mock(AddressInputMixin.AddressConverter::class.java)

    private val input = MutableStateFlow("")
    private val addressCandidates = MutableSharedFlow<AddressCandidates>(replay = 1)
    private val fundingDomainProvider = ParkedFundingDomainProvider()

    private val interactor = RealSendPaymentInteractor(
        chainAssetProvider = chainAssetProvider,
        fundingDomainProvider = fundingDomainProvider
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        whenever(chainAssetProvider.chainId()).thenReturn(CHAIN_ID)
        whenever(getContactsUseCase()).thenReturn(emptyList())
        withAddressConverters()
        whenever(addressInputMixin.input).thenReturn(input)
        whenever(addressInputMixin.addressCandidates).thenReturn(addressCandidates)
        whenever(addressInputMixinFactory.create(any(), any(), any())).thenReturn(addressInputMixin)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `shows Send to yourself before the first search result arrives`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        val state = viewModel.state.value

        assertTrue(state.sendToYourselfVisible)
        assertEquals(PaymentSearchResults.Loading, state.results)
    }

    @Test
    fun `shows Send to yourself for an empty input with recents listed`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        withRecentsListed(query = "")

        assertTrue(viewModel.awaitState { it.input.isEmpty() && it.results is PaymentSearchResults.Sections }.sendToYourselfVisible)
    }

    @Test
    fun `hides Send to yourself once the input has text`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onInputChange("an")
        withRecentsListed(query = "an")

        assertFalse(viewModel.awaitState { it.input == "an" && it.results is PaymentSearchResults.Sections }.sendToYourselfVisible)
    }

    @Test
    fun `shows Send to yourself again when the input is cleared`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onInputChange("an")
        withRecentsListed(query = "an")
        val typed = viewModel.awaitState { it.input == "an" && it.results is PaymentSearchResults.Sections }

        viewModel.onInputChange("")
        withRecentsListed(query = "")
        val cleared = viewModel.awaitState { it.input.isEmpty() && it.results is PaymentSearchResults.Sections }

        assertFalse(typed.sendToYourselfVisible)
        assertTrue(cleared.sendToYourselfVisible)
    }

    @Test
    fun `shows Send to yourself for an empty input when nobody is listed`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        withNobodyListed()

        assertTrue(viewModel.awaitState { it.results == PaymentSearchResults.Empty }.sendToYourselfVisible)
    }

    @Test
    fun `keeps Send to yourself when the typed text filters down to nothing`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onInputChange("  ")
        withNobodyListed()

        val state = viewModel.awaitState { it.results == PaymentSearchResults.Empty }
        assertEquals("", state.input)
        assertTrue(state.sendToYourselfVisible)
    }

    @Test
    fun `Send to yourself opens the withdraw sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onSendToYourselfClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedOnce(OFFRAMP_URL)
        router.verifySheetNeverOpened(ONRAMP_URL)
    }

    @Test
    fun `a second Send to yourself tap while the funding config loads opens one sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onSendToYourselfClick()
        viewModel.onSendToYourselfClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedOnce(OFFRAMP_URL)
    }

    @Test
    fun `after Send to yourself opened its sheet the next tap opens it again`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onSendToYourselfClick()
        fundingDomainProvider.completeReadsWithConfig()
        viewModel.onSendToYourselfClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedTwice(OFFRAMP_URL)
    }

    @Test
    fun `a failed funding config read shows the withdraw error and unlocks Send to yourself`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onSendToYourselfClick()
        fundingDomainProvider.failReads()
        viewModel.onSendToYourselfClick()
        fundingDomainProvider.completeReadsWithConfig()

        assertPresentationErrorShown<WithdrawUnavailablePresentationError>(viewModel)
        router.verifySheetOpenedOnce(OFFRAMP_URL)
    }

    private fun withAddressConverters() {
        whenever(previousPaymentsAddressConverterFactory.create(CHAIN_ID)).thenReturn(addressConverter)
        whenever(contactsAddressConverterFactory.create()).thenReturn(addressConverter)
        whenever(parseAddressConverterFactory.create(CHAIN_ID)).thenReturn(addressConverter)
        whenever(parseAddressUsernameConverterFactory.create(addressConverter)).thenReturn(addressConverter)
        whenever(usernameAddressConverterFactory.create()).thenReturn(addressConverter)
    }

    private suspend fun withRecentsListed(query: String) {
        addressCandidates.emit(
            AddressCandidates(
                query = query,
                local = listOf(
                    ExtractedAddressesSection(
                        category = ExtractedAddressesCategory.Custom(RCommon.string.search_section_recent),
                        addresses = listOf(address("anchorBay.17"))
                    )
                ),
                remote = RemoteSearchPhase.Loaded(emptyList())
            )
        )
    }

    private suspend fun withNobodyListed() {
        addressCandidates.emit(
            AddressCandidates(query = "", local = emptyList(), remote = RemoteSearchPhase.Loaded(emptyList()))
        )
    }

    private suspend fun SendPaymentViewModel.awaitState(predicate: (SendPaymentUiState) -> Boolean): SendPaymentUiState {
        return withTimeout(STATE_TIMEOUT) { state.first(predicate) }
    }

    private fun address(username: String) = ExtractedAddress(
        display = username,
        type = ExtractedAddress.DisplayType.USERNAME,
        accountId = username.encodeToByteArray().intoAccountId()
    )

    private fun createViewModel() = SendPaymentViewModel(
        walletRouter = router,
        clipboardService = clipboardService,
        getContactsUseCase = getContactsUseCase,
        chatStarter = chatStarter,
        addressInputMixinFactory = addressInputMixinFactory,
        usernameAddressConverterFactory = usernameAddressConverterFactory,
        parseAddressConverterFactory = parseAddressConverterFactory,
        parserAddressUsernameConverterFactory = parseAddressUsernameConverterFactory,
        previousPaymentsAddressConverterFactory = previousPaymentsAddressConverterFactory,
        contactsAddressConverterFactory = contactsAddressConverterFactory,
        interactor = interactor
    )
}
