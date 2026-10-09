import Foundation
import SubstrateSdk
import NovaCrypto
import Operation_iOS
import CommonService
import ExtrinsicService
import AssetExchange
import Individuality
import KeyDerivation
import ChainRegistry

protocol EvidenceSubmissionServiceProtocol: ApplicationServiceProtocol {
    func retry()
}

///  Class is designed to manage evidence submission process.
///
///  The full submission process consists of the following steps:
///      upload photo to bulletin chain -> submit photo hash to the people chain -> wait judging ->
///      -> allocate video storage -> upload video to the bulletin chain -> submit video hash to the people chain
///
///  Note: if there not many people applying then the people chain might decide to skip the photo judging phase
///  and allow to start from video uploading step.
///
///  Note: despite the fact that an evidence file is being uploaded by chunks it is currently fully loading to the
/// memory to calculate the hash.
///
///  The service monitors EvidenceSubmission.RemoteState subscribing to the remoteStateStore. It contains both candidate
/// state from People chain
///  and authorization from the Bulletin chain. Those data allows to understand at which stage the user is: for example,
/// uploading data or waiting the judgement.
///
///  However there is no way to precisely determine whether file was submitted fully or partially and whether the
/// service needs to wait submitted extrinsic.
///  To address the issues EvidenceSubmission.Session is introduced to save locally the progress. Also session contains
/// the error that a user needs to resolve.
///
///  To start the flow EvidenceSubmission.LocalState must be saved to the database. It contains information about
/// evidence files and session id to log progress to.
///   For example, it can be done from an interactor using EvidenceSubmissionStateRepositoryFactory after the all media
/// recorded are saved locally.
///
///  Also any interactor can subscribe to the EvidenceSubmission.Session to display current state of the submission or
/// request a user to resolve the error.
final class EvidenceSubmissionService {
    let wallet: WalletManaging
    let extrinsicOriginFactory: CandidateOriginFactoryProtocol
    let chainRegistry: ChainRegistryProtocol
    let remoteStateStore: BaseObservableStateStore<EvidenceSubmission.RemoteState>
    let localStateProviderFactory: EvidenceLocalDataProviderFactoryProtocol
    let sessionRepository: AnyDataProviderRepository<EvidenceSubmission.Session>
    let localStateRepository: AnyDataProviderRepository<EvidenceSubmission.LocalState>
    let userStorageFacade: StorageFacadeProtocol
    let substrateStorageFacade: StorageFacadeProtocol
    let fileManager: FileManager
    let evidenceFileManagerFactory: EvidenceFileManagerFactoryProtocol
    let maxChunkSize: UInt64
    let syncQueue: DispatchQueue
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    private(set) var session: EvidenceSubmission.Session?
    private(set) var remoteState: EvidenceSubmission.RemoteState?
    private(set) var localState: EvidenceSubmission.LocalState?
    private(set) var file: EvidenceSubmission.File?
    private(set) var localStateProvider: StreamableProvider<EvidenceSubmission.LocalState>?
    private(set) var peopleExtrinsicSubmissionFactory: ExtrinsicSubmitMonitorFactoryProtocol?
    private(set) var bulletinExtrinsicService: ExtrinsicServiceProtocol?

    init(
        wallet: WalletManaging,
        extrinsicOriginFactory: CandidateOriginFactoryProtocol,
        chainRegistry: ChainRegistryProtocol,
        remoteStateStore: BaseObservableStateStore<EvidenceSubmission.RemoteState>,
        localStateProviderFactory: EvidenceLocalDataProviderFactoryProtocol,
        repositoryFactory: EvidenceStateRepositoryFactoryProtocol,
        userStorageFacade: StorageFacadeProtocol,
        substrateStorageFacade: StorageFacadeProtocol,
        fileManager: FileManager,
        evidenceFileManagerFactory: EvidenceFileManagerFactoryProtocol,
        operationQueue: OperationQueue,
        maxChunkSize: UInt64 = UInt64(1.5 * 1_024.0 * 1_024.0), // 1.5 MB chunck size
        syncQueue: DispatchQueue = DispatchQueue(label: "io.polkadot.app.evidence.submission.\(UUID().uuidString)"),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.wallet = wallet
        self.extrinsicOriginFactory = extrinsicOriginFactory
        self.chainRegistry = chainRegistry
        self.remoteStateStore = remoteStateStore
        self.localStateProviderFactory = localStateProviderFactory
        localStateRepository = repositoryFactory.createLocalStateRepository()
        sessionRepository = repositoryFactory.createSessionRepository(for: nil)
        self.userStorageFacade = userStorageFacade
        self.substrateStorageFacade = substrateStorageFacade
        self.fileManager = fileManager
        self.evidenceFileManagerFactory = evidenceFileManagerFactory
        self.operationQueue = operationQueue
        self.maxChunkSize = maxChunkSize
        self.syncQueue = syncQueue
        self.logger = logger
    }

    func createExtrinsicService(
        for chainId: ChainModel.Id,
        extrinsicVersion: Extrinsic.FormatVersion
    ) -> ExtrinsicServiceProtocol? {
        let serviceFactory = ExtrinsicServiceFactory(
            chainRegistry: chainRegistry,
            substrateStorageFacade: substrateStorageFacade,
            customFeeEstimator: ExtrinsicCustomFeeEstimatorFactory(providers: []),
            transactionExtensionFactory: CompoundTxExtensionFactory(),
            extrinsicVersion: extrinsicVersion,
            operationQueue: operationQueue
        )

        guard let chain = chainRegistry.getChain(for: chainId) else {
            return nil
        }

        return try? serviceFactory.createExtrinsicService(
            chain: chain
        )
    }

    func createExtrinsicSubmissionMonitor(
        for chainId: ChainModel.Id
    ) -> ExtrinsicSubmitMonitorFactoryProtocol? {
        guard let chain = chainRegistry.getChain(for: chainId) else {
            return nil
        }
        return try? ExtrinsicSubmissionMonitorFacade(
            chainRegistry: chainRegistry,
            substrateStorageFacade: substrateStorageFacade,
            operationQueue: operationQueue,
            logger: logger
        ).createMonitorFactory(chain: chain)
    }

    func getPeopleChainExtrinsicSubmissionFactory() -> ExtrinsicSubmitMonitorFactoryProtocol? {
        if let peopleExtrinsicSubmissionFactory {
            return peopleExtrinsicSubmissionFactory
        }

        let factory = createExtrinsicSubmissionMonitor(for: AppConfig.Chains.usernameChain)
        peopleExtrinsicSubmissionFactory = factory

        return factory
    }

    func getBulletInChainExtrinsicService() -> ExtrinsicServiceProtocol? {
        if let bulletinExtrinsicService {
            return bulletinExtrinsicService
        }

        let service = createExtrinsicService(
            for: AppConfig.Chains.bulletInChain,
            extrinsicVersion: .V4
        )
        bulletinExtrinsicService = service

        return service
    }

    func calculatedRemainedSize(for chunks: [Data], currentIndex: Int) -> Int {
        chunks.suffix(chunks.count - currentIndex).reduce(0) { $0 + $1.count }
    }

    func loadChunks(for url: URL, maxChunkSize: UInt64) throws -> [Data] {
        let data = try Data(contentsOf: url)

        let chunks = data.chunked(by: Int(maxChunkSize))

        let chunkHashes = try chunks.map { try $0.blake2b32().toHex(includePrefix: true) }

        let hash = try data.blake2b32().toHex(includePrefix: true)

        let chunkInfo = EvidenceSubmission.ChunksInfo(
            chunks: chunkHashes,
            hash: hash,
            totalSize: data.count,
            path: url.lastPathComponent
        )

        let chunkInfoData = try JSONEncoder().encode(chunkInfo)

        return chunks + [chunkInfoData]
    }

    func getChunks(for fileMetadata: EvidenceSubmission.FileMetadata, maxChunkSize: UInt64) throws -> [Data] {
        if let file, file.metadata == fileMetadata {
            return file.chunks
        }

        let newChunks = try loadChunks(for: fileMetadata.fileUrl, maxChunkSize: maxChunkSize)

        file = .init(chunks: newChunks, metadata: fileMetadata)

        return newChunks
    }

    func subscribeRemoteState() {
        remoteStateStore.add(observer: self, queue: syncQueue) { [weak self] _, newRemoteState in
            self?.remoteState = newRemoteState
            self?.applyStates()
        }
    }

    func subscribeLocalState() {
        localStateProvider?.removeObserver(self)

        localStateProvider = localStateProviderFactory.createEvidenceSubmissionLocalState()
        localStateProvider?.addObserver(
            self,
            deliverOn: syncQueue,
            executing: { [weak self] changes in
                self?.localState = changes.reduceToLastChange()
                self?.session = nil

                if let localState = self?.localState {
                    self?.loadSessionAndApply(state: localState)
                }
            },
            failing: { [weak self] error in
                self?.logger.error("Did receive error: \(error)")
            },
            options: StreamableProviderObserverOptions()
        )
    }

    func clearLocalState() {
        if let localState {
            self.localState = nil

            let operation = localStateRepository.saveOperation({ [] }, { [localState.identifier] })
            operationQueue.addOperation(operation)
        }
    }

    func clearSession() {
        if let session {
            self.session = nil

            let operation = sessionRepository.saveOperation({ [] }, { [session.identifier] })
            operationQueue.addOperation(operation)
        }
    }

    func set(session: EvidenceSubmission.Session) {
        self.session = session
        save(session: session)
    }

    func updateSession(error: EvidenceSubmission.Session.Error?) {
        if let session {
            let newSession = session.changingError(error)
            self.session = newSession
            save(session: newSession)
        }
    }

    func updateSession(progress: EvidenceSubmission.Progress, mediaId: String) {
        if let session {
            let newSession = session
                .changingMediaId(mediaId)
                .changingProgress(progress)

            self.session = newSession
            save(session: newSession)
        }
    }

    func loadSessionAndApply(state: EvidenceSubmission.LocalState) {
        let operation = sessionRepository.fetchOperation(
            by: { state.sessionId },
            options: RepositoryFetchOptions()
        )

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: syncQueue
        ) { [weak self] result in
            switch result {
            case let .success(model):
                self?.session = model ?? EvidenceSubmission.Session(
                    identifier: state.sessionId,
                    mediaId: nil,
                    progress: nil,
                    error: nil
                )
                self?.applyStates()
            case let .failure(error):
                self?.logger.error("Progress fetch error: \(error)")
            }
        }
    }

    func save(session: EvidenceSubmission.Session) {
        let operation = sessionRepository.saveOperation({ [session] }, { [] })
        operationQueue.addOperation(operation)
    }

    func applyStates() {
        guard let session, session.error == nil else {
            // need to resolve the error first before proceding
            return
        }

        guard case let .selected(selectedModel) = remoteState?.candidate else {
            return
        }

        guard selectedModel.judging == nil else {
            applyEvidenceSubmissionComplete(for: selectedModel.allocation)
            return
        }

        guard let localState else {
            return
        }

        applySubmissionAllocation(
            for: localState,
            session: session,
            allocation: selectedModel.allocation,
            remote: remoteState
        )
    }

    func applyEvidenceSubmissionComplete(for allocation: ProofOfInkPallet.Allocation) {
        switch allocation {
        case .initial:
            if let session, session.mediaId != nil {
                let newSession = EvidenceSubmission.Session(identifier: session.identifier)

                logger.debug("Reseting session after evidence submission completion")

                set(session: newSession)
            }
        case .initDone:
            logger.warning("Photo submission must be completed when init done")
        case .full:
            logger.debug("Clear local state and session after submission")

            clearLocalState()
            clearSession()
        }
    }
}

extension EvidenceSubmissionService: EvidenceSubmissionServiceProtocol {
    func setup() {
        subscribeRemoteState()
        subscribeLocalState()
    }

    func retry() {
        guard let session, let serviceError = session.error else {
            return
        }

        switch serviceError {
        case .storeExtrinsic,
             .chunksLoading,
             .notEnoughStorage,
             .storageExpired:
            bulletinExtrinsicService = nil
            let newSession = session
                .clearingTxHash()
                .changingError(nil)
            set(session: newSession)
        case .submitHashExtrinsic,
             .allocateFull:
            peopleExtrinsicSubmissionFactory = nil
            let newSession = session
                .clearingTxHash()
                .changingError(nil)
            set(session: newSession)
        case .mediaMismatch:
            bulletinExtrinsicService = nil

            let newSession = EvidenceSubmission.Session(
                identifier: session.identifier,
                mediaId: nil,
                progress: session.progress,
                error: nil
            )

            set(session: newSession)
        }
    }

    func throttle() {
        remoteStateStore.remove(observer: self)

        localStateProvider?.removeObserver(self)
        localStateProvider = nil

        remoteState = nil
        localState = nil
        session = nil
    }
}
