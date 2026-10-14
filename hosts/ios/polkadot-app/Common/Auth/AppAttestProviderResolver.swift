import Foundation
import UniqueDevice

enum AppAttestProviderResolver {
    static func resolve() -> AppAttestProviding {
        #if DISABLE_AUTH
            return NoAppAttestProvider()
        #elseif targetEnvironment(simulator) && (DEBUG || E2E_TEST)
            // App Attest requires the Secure Enclave and can NEVER produce an
            // assertion on the Simulator. The proof-only
            // `SimulatorAppAttestProvider` is the only attest path that works
            // there. Use it for local DEBUG simulator development and for the
            // triangle-e2e artifact (E2E_TEST, a Nightly simulator build) so it
            // can onboard against the dev identity backend, mirroring how the
            // Android emulator uses Play Integrity for the same
            // proof-of-unique-device step. Real devices never satisfy
            // `targetEnvironment(simulator)`, so production / TestFlight builds
            // are unaffected, and a non-E2E Nightly simulator build keeps the
            // real attestation path so it can still be exercised during dev.
            return SimulatorAppAttestProvider()
        #else
            let storage = SubstrateDataStorageFacade.shared
            let repositoryFactory = AppAttestRepositoryFactory(storageFacade: storage)
            let providerFactory = AppAttestProviderFactory(
                repositoryFactory: repositoryFactory,
                operationQueue: OperationManagerFacade.sharedDefaultQueue
            )
            return providerFactory.createProvider(with: .appAttest)
        #endif
    }
}
