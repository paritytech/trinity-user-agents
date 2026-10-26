import AsyncExtensions
import Coinage
import Foundation

/// The installation the core derives NFT purse item keys under, once Coinage has backed it up, so a
/// reinstall can find the keys from the seed. Coinage is assembled after the runtime, so a call made
/// before ``attach(_:)`` waits for it.
final class TrUAPIBackedUpInstallation: @unchecked Sendable {
    private let attachedProvider = AsyncCurrentValueSubject<(any CoinageBackedUpInstallationProviding)?>(nil)

    func attach(_ provider: any CoinageBackedUpInstallationProviding) {
        attachedProvider.send(provider)
    }

    func installationId() async throws -> Data {
        for await provider in attachedProvider {
            guard let provider else { continue }
            return try await provider.backedUpInstallation().value
        }
        throw CancellationError()
    }
}
