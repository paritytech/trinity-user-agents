package io.paritytech.polkadotapp.feature_videogame_impl.domain.gameResults

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.storage.source.query.AtBlock
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountEcdhKey
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import io.paritytech.polkadotapp.feature_upgrade_username_api.domain.model.UpgradeUsernameAvailabilityState
import io.paritytech.polkadotapp.feature_upgrade_username_api.domain.usecase.CheckUsernameAvailabilityUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.StoredUsername
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.model.GameIndex
import io.paritytech.polkadotapp.feature_videogame_impl.data.ScoreContextProvider
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.GameNftsSubscriptionService
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainAccountOrPerson
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainEarlyAttendanceEnactment
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainGamePlayerCredibility
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainParticipant
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainPlayerDisposition
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameInfo
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGamePlayerInfo
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameRecognition
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameState
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameStreak
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.ScoreRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameRepositoryInternal
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.PlayingAccountUseCase
import junit.framework.TestCase.assertEquals
import junit.framework.TestCase.assertFalse
import junit.framework.TestCase.assertNull
import junit.framework.TestCase.assertTrue
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Test

/**
 * Unit tests for [RealGameResultsInteractor].
 *
 * Native-crypto seam: [AttestationContextResolver] calls the top-level extension
 * `BandersnatchSecretsStorage.getAliasInContext`, which derives a bandersnatch alias via the
 * JNI `BandersnatchCrypto.alias_in_context`. That native lib is Android-only (no host build), so
 * a plain JVM unit test cannot run it. We intercept the *Kotlin extension* (compiled into the
 * file-class `BandersnatchEntropyKt`) with `mockkStatic`, stubbing `aliasInContext` to return a
 * fake alias while everything else runs for real — this never references `BandersnatchCrypto`,
 * so the native lib is never loaded. [withFakeAlias] wraps every test that
 * needs the CHAIN context to resolve.
 */
class RealGameResultsInteractorTest {
    private val playingAccountUseCase: PlayingAccountUseCase = mockk()
    private val accountRepository: AccountRepository = mockk()
    private val chainRegistry: ChainRegistry = mockk()
    private val resourcesRepository: ResourcesRepository = mockk()
    private val videoGameRepository: VideoGameRepositoryInternal = mockk()
    private val gameGroupRosterService: GameGroupRosterService = mockk()
    private val gameNftsSubscriptionService: GameNftsSubscriptionService = mockk()
    private val airdropPrizeService: AirdropPrizeService = mockk()
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage = mockk()
    private val reportSnapshot: GameReportSnapshot = mockk()
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase = mockk()
    private val checkUsernameAvailabilityUseCase: CheckUsernameAvailabilityUseCase = mockk()

    private val scoreRepository: ScoreRepository = mockk()
    private val scoreContextProvider: ScoreContextProvider = mockk()

    // Real collaborators wired around the same mocks — keeps every assertion driving the actual
    // context-resolve and live-update paths end-to-end, rather than stubbing the extracted seams.
    private val attestationContextResolver = AttestationContextResolver(
        playingAccountUseCase = playingAccountUseCase,
        chainRegistry = chainRegistry,
        videoGameRepository = videoGameRepository,
        gameGroupRosterService = gameGroupRosterService,
        bandersnatchSecretsStorage = bandersnatchSecretsStorage,
        reportSnapshot = reportSnapshot,
        scoreContextProvider = scoreContextProvider,
    )

    private val gameResultsLiveUpdater = GameResultsLiveUpdater(
        attestationContextResolver = attestationContextResolver,
        gameNftsSubscriptionService = gameNftsSubscriptionService,
        videoGameRepository = videoGameRepository,
        airdropPrizeService = airdropPrizeService,
        reportSnapshot = reportSnapshot,
        scoreRepository = scoreRepository
    )

    private val interactor = RealGameResultsInteractor(
        playingAccountUseCase = playingAccountUseCase,
        accountRepository = accountRepository,
        chainRegistry = chainRegistry,
        resourcesRepository = resourcesRepository,
        videoGameRepository = videoGameRepository,
        airdropPrizeService = airdropPrizeService,
        reportSnapshot = reportSnapshot,
        usernameOfAccountUseCase = usernameOfAccountUseCase,
        checkUsernameAvailabilityUseCase = checkUsernameAvailabilityUseCase,
        attestationContextResolver = attestationContextResolver,
        gameResultsLiveUpdater = gameResultsLiveUpdater,
        scoreRepository = scoreRepository
    )

    // region — ATTESTATIONS (CHAIN path) ------------------------------------------------------

    @Test
    fun `total is the fixed pack size and score is capped at it`() = withFakeAlias {
        // 12 minted, capped at the contract pack size (10).
        val candidates = (1..12).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 7,
            expectedPeerRounds = 12,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
        )

        val result = build()

        assertEquals(10, result.attestations.total)
        assertEquals(10, result.attestations.score)
    }

    @Test
    fun `score is the real minted count when below total`() = withFakeAlias {
        val candidates = (1..4).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 10,
            expectedPeerRounds = 4,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
        )

        val result = build()

        assertEquals(10, result.attestations.total)
        assertEquals(4, result.attestations.score)
    }

    @Test
    fun `passed is true when at least the threshold of attestations minted`() = withFakeAlias {
        val candidates = (1..6).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 10,
            expectedPeerRounds = 6,
            earlyAttendance = null,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
        )

        assertTrue(build().attestations.passed)
    }

    @Test
    fun `passed is false when fewer than the threshold minted`() = withFakeAlias {
        val candidates = (1..5).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 10,
            expectedPeerRounds = 5,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
        )

        assertFalse(build().attestations.passed)
    }

    @Test
    fun `passed game streams the minted then completes the pack from pending`() = withFakeAlias {
        val candidates = (1..6).map { byteArrayOf(it.toByte()).toDataByteArray() }
        val pending = (7..10).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 12,
            expectedPeerRounds = 6,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
            pendingHashes = pending,
        )

        val result = build()

        assertTrue(result.attestations.passed)
        assertEquals(10, result.attestationHashes.size)
        assertEquals(
            (candidates + pending).map { it.value.joinToString("") { b -> "%02x".format(b) } },
            result.attestationHashes
        )
    }

    @Test
    fun `passing player below the pack size is completed from pending, never synthetic`() = withFakeAlias {
        val candidates = (1..6).map { byteArrayOf(it.toByte()).toDataByteArray() }
        val pending = (7..8).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 8,
            expectedPeerRounds = 6,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
            pendingHashes = pending,
        )

        val result = build()

        assertEquals(8, result.attestationHashes.size)
        assertEquals(
            (candidates + pending).map { it.value.joinToString("") { b -> "%02x".format(b) } },
            result.attestationHashes
        )
    }

    @Test
    fun `failed game streams only the minted candidate hashes`() = withFakeAlias {
        val candidates = (1..5).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 5,
            expectedPeerRounds = 5,
            earlyAttendance = false,
            candidateHashes = candidates,
            mintedHashes = candidates.take(3).toSet(),
        )

        val result = build()

        assertFalse(result.attestations.passed)
        assertEquals(
            candidates.take(3).map { it.value.joinToString("") { b -> "%02x".format(b) } },
            result.attestationHashes
        )
    }

    @Test
    fun `live results upgrade to passed when the count reaches the threshold`() = withFakeAlias {
        val candidates = (1..6).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(rounds = 6, expectedPeerRounds = 6, earlyAttendance = null, candidateHashes = candidates)
        every { gameNftsSubscriptionService.newlyMintedHashes(any(), any(), any()) } returns
            flowOf(*candidates.toTypedArray())
        val initial = build()

        val events = collectLiveResults(initial)

        assertEquals(6, events.filterIsInstance<GameResultsLiveEvent.AttestationMinted>().size)
        val upgrade = events.filterIsInstance<GameResultsLiveEvent.UpgradedToPassed>().single()
        assertTrue(upgrade.input.attestations.passed)
        assertEquals(0, upgrade.padHashes.size)
        assertEquals(6, upgrade.input.attestationHashes.size)
    }

    @Test
    fun `live results only stream mints while the count stays below the threshold`() = withFakeAlias {
        val candidates = (1..3).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(rounds = 3, expectedPeerRounds = 3, earlyAttendance = null, candidateHashes = candidates)
        every { gameNftsSubscriptionService.newlyMintedHashes(any(), any(), any()) } returns
            flowOf(candidates.first(), candidates[1])
        val initial = build()

        val events = collectLiveResults(initial)

        assertEquals(2, events.filterIsInstance<GameResultsLiveEvent.AttestationMinted>().size)
        assertTrue(events.filterIsInstance<GameResultsLiveEvent.UpgradedToPassed>().isEmpty())
    }

    // endregion

    // region — FALLBACKS to the empty result -------------------------------------------------

    @Test
    fun `gameInfo null yields the empty not-passed result`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true, gameInfo = null)

        val result = build()

        assertEquals(Attestations(score = 0, total = 10, passed = false), result.attestations)
        assertTrue(result.attestationHashes.isEmpty())
    }

    @Test
    fun `player data null yields the empty not-passed result`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true, playerInfo = null)

        assertEquals(Attestations(score = 0, total = 10, passed = false), build().attestations)
    }

    @Test
    fun `playerCount null yields the empty not-passed result`() = withFakeAlias {
        // Game state not Reporting, and no report snapshot => playerCount null => empty result.
        stubChainContext(
            rounds = 10,
            expectedPeerRounds = 8,
            earlyAttendance = true,
            gameState = OnChainVideoGameState.PlayerProcess,
            reportContext = null,
        )

        assertEquals(Attestations(score = 0, total = 10, passed = false), build().attestations)
    }

    @Test
    fun `roster computeCandidates failure yields the empty not-passed result`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        coEvery {
            gameGroupRosterService.computeCandidates(any(), any(), any(), any(), any(), any())
        } returns Result.failure(RuntimeException("roster boom"))

        assertEquals(Attestations(score = 0, total = 10, passed = false), build().attestations)
    }

    @Test
    fun `playerCount falls back to report snapshot when game state is not Reporting`() = withFakeAlias {
        // PlayerProcess => Reporting.playerCount unavailable, but snapshot has it => CHAIN path resolves.
        val candidates = (1..5).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 5,
            expectedPeerRounds = 5,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
            gameState = OnChainVideoGameState.PlayerProcess,
            reportContext = GameReportSnapshot.ReportContext(wasRegistered = true, playerCount = 12),
        )

        val result = build()

        assertEquals(5, result.attestations.score)
    }

    // endregion

    // region — PRIZE DRAW --------------------------------------------------------------------

    @Test
    fun `prize draw is null when not passed`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 9, earlyAttendance = false)
        stubMember()

        assertNull(build().prizeDraw)
    }

    @Test
    fun `prize draw is null when passed but not a member`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()

        assertNull(build().prizeDraw)
    }

    @Test
    fun `prize draw is fetched when passed and a member`() = withFakeAlias {
        val candidates = (1..6).map { byteArrayOf(it.toByte()).toDataByteArray() }
        stubChainContext(
            rounds = 10,
            expectedPeerRounds = 6,
            earlyAttendance = true,
            candidateHashes = candidates,
            mintedHashes = candidates.toSet(),
        )
        stubMember()
        val fetched = PrizeDraw(
            prizeUsd = 200.toBigDecimal(),
            userTicket = "a".repeat(64),
            winningTickets = listOf("b".repeat(64)),
            ticketDistance = 0L,
            totalEntries = 100L,
            nextDrawAt = "2026-06-08T00:00:00Z",
            won = true,
        )
        coEvery { airdropPrizeService.fetchPrizeDraw(any(), any(), any()) } returns Result.success(fetched)

        assertEquals(fetched, build().prizeDraw)
    }

    @Test
    fun `prize draw is null when chain context is null`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true, gameInfo = null)
        stubMember()

        assertNull(build().prizeDraw)
    }

    // endregion

    // region — MEMBER ------------------------------------------------------------------------

    @Test
    fun `justBecameMember is true when snapshot says not previously registered`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubConsumer(fullUsername = "byteboro", liteUsername = "byteboro.42")
        stubMember()
        every { reportSnapshot.current() } returns
            GameReportSnapshot.ReportContext(wasRegistered = false, playerCount = 12)

        assertTrue(build().member.justBecameMember)
    }

    @Test
    fun `justBecameMember is false when previously registered or snapshot missing`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubConsumer(fullUsername = "byteboro", liteUsername = "byteboro.42")
        every { reportSnapshot.current() } returns null

        assertFalse(build().member.justBecameMember)
    }

    @Test
    fun `display name is the consumer username when consumer present`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubConsumer(fullUsername = "byteboro", liteUsername = "byteboro.42")

        assertEquals("byteboro", build().member.displayName)
    }

    @Test
    fun `display name falls back to local username when consumer absent`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()
        stubLocalUsername(Username.fromParts(username = "localhandle", index = 7))

        assertEquals("localhandle.07", build().member.displayName)
    }

    @Test
    fun `display name is null when both consumer and local username absent`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()
        stubLocalUsername(null)

        assertNull(build().member.displayName)
    }

    // endregion

    // region — USERNAME CLAIM ----------------------------------------------------------------

    @Test
    fun `consumer with full username is not eligible`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubConsumer(fullUsername = "byteboro", liteUsername = "byteboro.42")

        assertFalse(build().usernameClaim.eligible)
    }

    @Test
    fun `consumer with lite username only is eligible with base suggestion`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubConsumer(fullUsername = null, liteUsername = "byteboro.42")

        val claim = build().usernameClaim

        assertTrue(claim.eligible)
        assertEquals("byteboro", claim.suggestedUsername)
    }

    @Test
    fun `local lite username with index is eligible`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()
        stubLocalUsername(Username.fromParts(username = "localhandle", index = 3))

        val claim = build().usernameClaim

        assertTrue(claim.eligible)
        assertEquals("localhandle", claim.suggestedUsername)
    }

    @Test
    fun `local username with no index is not eligible`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()
        stubLocalUsername(Username.fromParts(username = "fullhandle", index = null))

        assertFalse(build().usernameClaim.eligible)
    }

    @Test
    fun `no consumer and no local username is not eligible`() = withFakeAlias {
        stubChainContext(rounds = 10, expectedPeerRounds = 8, earlyAttendance = true)
        stubNoConsumer()
        stubLocalUsername(null)

        assertFalse(build().usernameClaim.eligible)
    }

    // endregion

    // region — resolveUsernameAvailability ---------------------------------------------------
    // The queue-walking logic now lives in CheckUsernameAvailabilityUseCase (upgrade-username);
    // here we only verify the use-case state is mapped to the webview enum.

    @Test
    fun `availability maps Free to AVAILABLE`() = runBlocking<Unit> {
        coEvery { checkUsernameAvailabilityUseCase("byteboro") } returns
            Result.success(UpgradeUsernameAvailabilityState.Free)

        assertEquals(UsernameAvailability.AVAILABLE, resolveAvailability("byteboro"))
    }

    @Test
    fun `availability maps ReservedByUs to AVAILABLE`() = runBlocking<Unit> {
        coEvery { checkUsernameAvailabilityUseCase("a") } returns
            Result.success(UpgradeUsernameAvailabilityState.ReservedByUs)

        assertEquals(UsernameAvailability.AVAILABLE, resolveAvailability("a"))
    }

    @Test
    fun `availability maps NotAvailable to TAKEN`() = runBlocking<Unit> {
        coEvery { checkUsernameAvailabilityUseCase("byteboro") } returns
            Result.success(UpgradeUsernameAvailabilityState.NotAvailable)

        assertEquals(UsernameAvailability.TAKEN, resolveAvailability("byteboro"))
    }

    @Test
    fun `availability is UNKNOWN when the use-case fails`() = runBlocking<Unit> {
        coEvery { checkUsernameAvailabilityUseCase("byteboro") } returns Result.failure(RuntimeException("rpc down"))

        assertEquals(UsernameAvailability.UNKNOWN, resolveAvailability("byteboro"))
    }

    // endregion

    // region — test harness ------------------------------------------------------------------

    private fun build(): GameResultsInput = runBlocking {
        val scope = ComputationalScope(this)
        with(scope) { interactor.buildGameResults() }
    }

    private fun resolveAvailability(name: String): UsernameAvailability = runBlocking {
        val scope = ComputationalScope(this)
        with(scope) { interactor.resolveUsernameAvailability(name) }
    }

    private fun collectLiveResults(initial: GameResultsInput): List<GameResultsLiveEvent> = runBlocking {
        val scope = ComputationalScope(this)
        with(scope) { interactor.subscribeLiveResults(initial).toList() }
    }

    /**
     * Stubs every collaborator so the CHAIN attestation context resolves. Individual tests
     * override the pieces they care about; passing `gameInfo`/`playerInfo` = null (or a
     * non-Reporting [gameState] with `reportContext` = null) drives the mock-fallback branches.
     */
    @Suppress("LongParameterList")
    private fun stubChainContext(
        rounds: Int,
        expectedPeerRounds: Int,
        earlyAttendance: Boolean?,
        candidateHashes: List<DataByteArray> = listOf(byteArrayOf(1).toDataByteArray()),
        mintedHashes: Set<DataByteArray> = emptySet(),
        pendingHashes: List<DataByteArray> = emptyList(),
        gameInfo: OnChainVideoGameInfo? = defaultGameInfo(rounds, OnChainVideoGameState.Reporting(playerCount = 12)),
        playerInfo: OnChainVideoGamePlayerInfo? = defaultPlayer(earlyAttendance),
        gameState: OnChainVideoGameState? = null,
        reportContext: GameReportSnapshot.ReportContext? =
            GameReportSnapshot.ReportContext(wasRegistered = true, playerCount = 12),
    ) {
        val resolvedGameInfo = when {
            gameInfo == null -> null
            gameState != null -> defaultGameInfo(rounds, gameState)
            else -> gameInfo
        }

        coEvery { chainRegistry.peopleChain() } returns peopleChain
        coEvery { bandersnatchSecretsStorage.getEntropy(any()) } returns BandersnatchEntropy(ByteArray(32))
        coEvery { scoreContextProvider.context() } returns BandersnatchContext(ByteArray(0))

        // subscribeOurPlayer -> subscribeAccountPlayer/subscribePersonPlayer -> subscribePlayer
        every { videoGameRepository.subscribePlayer(any(), any()) } returns flowOf(null)
        every {
            videoGameRepository.subscribePlayer(any(), OnChainAccountOrPerson.Account(OUR_ACCOUNT_ID))
        } returns flowOf(playerInfo)
        // The fresh remote row mirrors the cached one by default; staleness tests override.
        coEvery { videoGameRepository.getPlayer(any(), any()) } returns Result.success(playerInfo)

        // These collaborators carry a context(ComputationalScope) receiver, which the compiler
        // lowers to a leading parameter — so the receiver must itself be matched with any().
        coEvery { with(any<ComputationalScope>()) { playingAccountUseCase.getPlayingAccount() } } returns playingAccount
        coEvery { with(any<ComputationalScope>()) { playingAccountUseCase.getOurPlayerAccountId() } } returns
            OUR_ACCOUNT_ID
        // subscribeGameInfo -> subscribeGameInfoAtBlock
        every {
            with(any<ComputationalScope>()) { videoGameRepository.subscribeGameInfoAtBlock(any()) }
        } returns gameInfoFlow(resolvedGameInfo)

        every { reportSnapshot.current() } returns reportContext

        coEvery {
            gameGroupRosterService.computeCandidates(any(), any(), any(), any(), any(), any())
        } returns Result.success(
            GameAttestationCandidates(hashes = candidateHashes, expectedPeerRounds = expectedPeerRounds)
        )

        coEvery {
            videoGameRepository.getMintedNftHashes(any(), any(), any(), any())
        } returns Result.success(mintedHashes)

        coEvery {
            videoGameRepository.getPendingNftHashes(any(), any(), any())
        } returns Result.success(pendingHashes)

        // Default: not a member, no stored username (tests override as needed).
        coEvery { resourcesRepository.consumerInfo(any(), any()) } returns Result.success(null)
        coEvery { usernameOfAccountUseCase.getUsername() } returns Result.success(null)
        coEvery { scoreRepository.getParticipantFresh(any(), any()) } returns Result.success(null)
    }

    private fun stubMember() {
        coEvery { scoreRepository.getParticipantFresh(any(), any()) } returns Result.success(recognizedParticipant())
    }

    private fun stubNoConsumer() {
        coEvery { resourcesRepository.consumerInfo(any(), any()) } returns Result.success(null)
    }

    private fun stubLocalUsername(username: Username?) {
        val stored = username?.let { StoredUsername(fullUsername = null, liteUsername = it, isOnChain = false) }
        coEvery { usernameOfAccountUseCase.getUsername() } returns Result.success(stored)
    }

    private fun stubConsumer(fullUsername: String?, liteUsername: String) {
        coEvery { resourcesRepository.consumerInfo(any(), any()) } returns Result.success(
            ConsumerInfo(
                accountId = OUR_ACCOUNT_ID,
                identifierKey = AccountEcdhKey.Unknown(byteArrayOf(7).toDataByteArray()),
                fullUsername = fullUsername,
                liteUsername = liteUsername,
            )
        )
    }

    // A member: recognition.isRecognized() is true for ExternallyRecognized — enough for isMember.
    private fun recognizedParticipant() = OnChainParticipant(
        score = 0,
        streak = OnChainVideoGameStreak.Attended(count = 1),
        credit = Balance.ZERO,
        cashedOut = false,
        reachedPersonhoodLastAttendance = false,
        hasEverReachedPersonhood = false,
        recognition = OnChainVideoGameRecognition.ExternallyRecognized,
    )

    private fun defaultPlayer(earlyAttendance: Boolean?): OnChainVideoGamePlayerInfo {
        val enactment = earlyAttendance?.let {
            OnChainEarlyAttendanceEnactment(attendance = it, disposition = OnChainPlayerDisposition.Keep)
        }
        return OnChainVideoGamePlayerInfo(
            firstGame = GameIndex(1),
            registered = true,
            sentReport = true,
            earlyAttendanceEnactment = enactment,
            yesPerson = 1,
            noNotPerson = 0,
            expectedMaxVoteWeight = 10,
            voteWeight = 5,
            credibility = OnChainGamePlayerCredibility.Recognized,
        )
    }

    private fun defaultGameInfo(rounds: Int, state: OnChainVideoGameState) = OnChainVideoGameInfo(
        index = GameIndex(1),
        registrationEnds = 0L,
        gameDate = 0L,
        reportEnds = 0L,
        maxGroupSize = 4,
        rounds = rounds,
        state = state,
        airdropsScheduled = 0,
    )

    private fun gameInfoFlow(gameInfo: OnChainVideoGameInfo?): Flow<AtBlock<OnChainVideoGameInfo?>> =
        flowOf(AtBlock(value = gameInfo, at = "0xblock"))

    /**
     * Runs [block] with the bandersnatch alias extension faked out, so the CHAIN path can resolve
     * without the Android-only JNI lib. See class KDoc.
     */
    private fun withFakeAlias(block: () -> Unit) {
        mockkStatic(BANDERSNATCH_ENTROPY_KT)
        try {
            every { any<BandersnatchEntropy>().aliasInContext(any()) } returns BandersnatchAlias(ByteArray(32))
            block()
        } finally {
            unmockkStatic(BANDERSNATCH_ENTROPY_KT)
        }
    }

    private val peopleChain: Chain = mockk<Chain>().also {
        every { it.id } returns CHAIN_ID
    }

    private val playingAccount: MetaAccount = mockk<MetaAccount>().also {
        every { it.id } returns META_ID
    }

    private companion object {
        const val CHAIN_ID = "people-chain-id"
        const val META_ID = 42L
        const val BANDERSNATCH_ENTROPY_KT = "io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropyKt"
        val OUR_ACCOUNT_ID: AccountId = ByteArray(32) { 1 }.toDataByteArray()
    }
    // endregion
}
