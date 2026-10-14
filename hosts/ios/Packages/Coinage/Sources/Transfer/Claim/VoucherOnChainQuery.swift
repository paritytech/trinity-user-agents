import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import Individuality

// MARK: - Result type

struct VoucherOnChainInfo {
    let publicKey: Data
    let exponent: Int16
    let ringPosition: MembersPallet.RingPosition
    /// Included member count of the ring this voucher was placed in (`RingKeysStatus.included`); nil when
    /// the voucher holds no ring index (Onboarding/Suspended) or the ring status could not be read.
    let ringMembersCount: UInt32?
    /// Three-valued: a Suspended member (no ring index to key an alias under) or a failed alias read
    /// leaves consumption `.unknown` rather than falsely reading not-unloaded.
    let aliasPresence: VoucherAliasPresence

    var isUnloaded: Bool { aliasPresence == .unloaded }

    /// The reconciled on-chain location: an unloaded read overrides everything, a ring-placed voucher
    /// becomes in-recycler carrying its real member count, otherwise it is still onboarding.
    var onChainState: Voucher.OnChainState {
        guard !isUnloaded else { return .unlocated }
        guard let ringIndex = ringPosition.ringIndex else { return .onboarding }
        return .inRecycler(Voucher.Recycler(index: ringIndex, membersCount: ringMembersCount ?? 0))
    }
}

// MARK: - Protocol

/// Batch-fetches on-chain voucher state for given derivation indices.
protocol VoucherOnChainQuerying: Sendable {
    /// Fetches on-chain voucher info for multiple derivation indices in a single RPC call.
    /// Returns an array of optionals in the same order as the input indices.
    /// Returns nil for an index when either the recycler location or member record is absent.
    func fetchVouchers(
        for derivationIndices: [CoinageKeyIndex],
        atBlockHash: Data?
    ) async throws -> [VoucherOnChainInfo?]
}

extension VoucherOnChainQuerying {
    func fetchVouchers(
        for derivationIndices: [CoinageKeyIndex]
    ) async throws -> [VoucherOnChainInfo?] {
        try await fetchVouchers(for: derivationIndices, atBlockHash: nil)
    }
}

// MARK: - Implementation

/// Default implementation that queries the Recyclers and Members pallet storage via RPC.
final class VoucherOnChainQueryService: VoucherOnChainQuerying, @unchecked Sendable {
    private let instanceId: CoinageInstanceId
    private let connection: any JSONRPCEngine
    private let runtimeService: any RuntimeCodingServiceProtocol
    private let storageRequestFactory: any StorageRequestFactoryProtocol
    private let publicKeyProvider: (CoinageKeyIndex) throws -> Data
    private let aliasProvider: (CoinageKeyIndex) throws -> Data

    init(
        instanceId: CoinageInstanceId,
        connection: any JSONRPCEngine,
        runtimeService: any RuntimeCodingServiceProtocol,
        storageRequestFactory: any StorageRequestFactoryProtocol,
        publicKeyProvider: @escaping (CoinageKeyIndex) throws -> Data,
        aliasProvider: @escaping (CoinageKeyIndex) throws -> Data
    ) {
        self.instanceId = instanceId
        self.connection = connection
        self.runtimeService = runtimeService
        self.storageRequestFactory = storageRequestFactory
        self.publicKeyProvider = publicKeyProvider
        self.aliasProvider = aliasProvider
    }

    func fetchVouchers(
        for derivationIndices: [CoinageKeyIndex],
        atBlockHash: Data?
    ) async throws -> [VoucherOnChainInfo?] {
        typealias IndexedKey = (index: CoinageKeyIndex, publicKey: Data)
        typealias IndexedKeyWithExponent = (index: CoinageKeyIndex, publicKey: Data, exponent: Int16)
        typealias IndexedKeyWithPosition = (
            index: CoinageKeyIndex,
            publicKey: Data,
            exponent: Int16,
            ringPosition: MembersPallet.RingPosition
        )

        guard !derivationIndices.isEmpty else { return [] }

        // Step 1: derive public keys — carry (index, publicKey) forward
        let indexedKeys: [IndexedKey] = try derivationIndices.map {
            try (index: $0, publicKey: publicKeyProvider($0))
        }

        // Step 2: fetch exponents — drop indices without one, carry (index, publicKey, exponent) forward
        let exponents = try await fetchExponents(
            for: indexedKeys.map(\.publicKey),
            atBlockHash: atBlockHash
        )
        let withExponents: [IndexedKeyWithExponent] =
            zip(indexedKeys, exponents).compactMap { key, exponent in
                guard let exponent else { return nil }
                return (key.index, key.publicKey, exponent)
            }

        // Step 3: fetch positions. A recycler member is present whatever its ring position — Onboarding
        // and Suspended included — so only a member with no position row at all is dropped (its presence
        // is unknown). A non-member was already dropped at step 2. This mirrors the durability reading
        // where archival (loss of membership) is never read as absence.
        let positions = try await fetchPositions(
            for: withExponents.map { (exponent: $0.exponent, publicKey: $0.publicKey) },
            atBlockHash: atBlockHash
        )
        let members: [IndexedKeyWithPosition] =
            zip(withExponents, positions).compactMap { key, position in
                guard let position else { return nil }
                return (key.index, key.publicKey, key.exponent, position)
            }

        // Step 4: alias states are keyed by ring index, so only members placed in a ring (Included) can
        // carry one. An alias read that fails leaves only those ring-placed vouchers `.unknown`;
        // Onboarding and Suspended verdicts come from the position alone, so a failed alias never erases
        // what a position already proves.
        let placed = members.compactMap {
            member -> (derivationIndex: CoinageKeyIndex, exponent: Int16, ringIndex: MembersPallet.RingIndex)? in
            guard let ringIndex = member.ringPosition.ringIndex else { return nil }
            return (member.index, member.exponent, ringIndex)
        }

        let aliasFetchSucceeded: Bool
        var aliasByIndex: [CoinageKeyIndex: CoinagePallet.AliasState?] = [:]
        if let aliasStates = try? await fetchAliasStates(for: placed, atBlockHash: atBlockHash) {
            aliasFetchSucceeded = true
            for (key, aliasState) in zip(placed, aliasStates) {
                aliasByIndex[key.derivationIndex] = aliasState
            }
        } else {
            aliasFetchSucceeded = false
        }

        // Step 5: read each placed ring's included member count so the recovered voucher carries the real
        // ring size rather than a placeholder. A ring with no status row simply leaves the count nil.
        let ringMembersByIndex = try await fetchRingMemberCounts(for: placed, atBlockHash: atBlockHash)

        let infoByIndex: [CoinageKeyIndex: VoucherOnChainInfo] = members.reduce(into: [:]) { dict, member in
            dict[member.index] = VoucherOnChainInfo(
                publicKey: member.publicKey,
                exponent: member.exponent,
                ringPosition: member.ringPosition,
                ringMembersCount: ringMembersByIndex[member.index],
                aliasPresence: Self.aliasPresence(
                    for: member.ringPosition,
                    aliasState: aliasByIndex[member.index] ?? nil,
                    aliasFetchSucceeded: aliasFetchSucceeded
                )
            )
        }

        return derivationIndices.map { infoByIndex[$0] }
    }

    /// The three-valued unload reading for one voucher. Onboarding never held a ring index, so no unload
    /// was possible — provably not-unloaded without a read. Suspended once did but holds none now, so its
    /// alias key cannot be formed and nothing can be said. Included reads the alias, unless that read
    /// failed.
    private static func aliasPresence(
        for position: MembersPallet.RingPosition,
        aliasState: CoinagePallet.AliasState?,
        aliasFetchSucceeded: Bool
    ) -> VoucherAliasPresence {
        if position.isOnboarding { return .notUnloaded }
        guard position.ringIndex != nil else { return .unknown }
        guard aliasFetchSucceeded else { return .unknown }
        return aliasState == .unloaded ? .unloaded : .notUnloaded
    }
}

// MARK: - Storage value

/// Value of `Coinage.RecyclersCoinToRecycler`: the instance the recycler belongs to and its denomination.
private struct RecyclerAssignment: Decodable {
    let instanceId: CoinageInstanceId
    let exponent: Int16

    init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        instanceId = try container.decode(StringCodable<CoinageInstanceId>.self).wrappedValue
        exponent = try container.decode(StringCodable<Int16>.self).wrappedValue
    }
}

// MARK: - NMap key

private struct RecyclerAliasStateKey: NMapKeyStorageKeyProtocol {
    let instanceId: CoinageInstanceId
    let exponent: Int16
    let ringIndex: MembersPallet.RingIndex
    let publicKey: Data

    func appendSubkey(to encoder: any DynamicScaleEncoding, type: String, index: Int) throws {
        switch index {
        case 0:
            try encoder.append(StringCodable(wrappedValue: instanceId), ofType: type)
        case 1:
            try encoder.append(StringCodable(wrappedValue: exponent), ofType: type)
        case 2:
            try encoder.append(StringCodable(wrappedValue: ringIndex), ofType: type)
        case 3:
            try encoder.append(BytesCodable(wrappedValue: publicKey), ofType: type)
        default:
            break
        }
    }
}

// MARK: - Private queries

private extension VoucherOnChainQueryService {
    func fetchExponents(
        for publicKeys: [Data],
        atBlockHash: BlockHashData?
    ) async throws -> [Int16?] {
        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        let responses: [RecyclerAssignment?] = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams: { publicKeys.map { BytesCodable(wrappedValue: $0) } },
            factory: { coderFactory },
            storagePath: CoinagePallet.Storage.recyclersCoinToRecycler(),
            at: atBlockHash
        )
        .asyncExecute()
        .map(\.value)

        return responses.map { assignment in
            guard let assignment, assignment.instanceId == instanceId else { return nil }
            return assignment.exponent
        }
    }

    func fetchAliasStates(
        for keys: [(derivationIndex: CoinageKeyIndex, exponent: Int16, ringIndex: MembersPallet.RingIndex)],
        atBlockHash: Data?
    ) async throws -> [CoinagePallet.AliasState?] {
        guard !keys.isEmpty else { return [] }

        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        let nMapKeys: [RecyclerAliasStateKey] = try keys.map {
            let alias = try aliasProvider($0.derivationIndex)
            return RecyclerAliasStateKey(
                instanceId: instanceId,
                exponent: $0.exponent,
                ringIndex: $0.ringIndex,
                publicKey: alias
            )
        }

        return try await storageRequestFactory
            .queryNMapItems(
                engine: connection,
                nParamKeys: { nMapKeys },
                factory: { coderFactory },
                storagePath: CoinagePallet.Storage.recyclerAliasStates(),
                at: atBlockHash
            )
            .asyncExecute()
            .map(\.value)
    }

    func fetchRingMemberCounts(
        for keys: [(derivationIndex: CoinageKeyIndex, exponent: Int16, ringIndex: MembersPallet.RingIndex)],
        atBlockHash: BlockHashData?
    ) async throws -> [CoinageKeyIndex: UInt32] {
        guard !keys.isEmpty else { return [:] }

        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        let statuses: [MembersPallet.RingKeysStatus?] = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams1: { [instanceId] in
                keys.map { RecyclerCollectionIdentifier.identifier(instanceId: instanceId, for: $0.exponent) }
            },
            keyParams2: {
                keys.map { StringCodable(wrappedValue: $0.ringIndex) }
            },
            factory: { coderFactory },
            storagePath: MembersPallet.Storage.ringKeysStatus(),
            at: atBlockHash
        )
        .asyncExecute()
        .map(\.value)

        return zip(keys, statuses).reduce(into: [:]) { dict, pair in
            guard let status = pair.1 else { return }
            dict[pair.0.derivationIndex] = status.included
        }
    }

    func fetchPositions(
        for keys: [(exponent: Int16, publicKey: Data)],
        atBlockHash: BlockHashData?
    ) async throws -> [MembersPallet.RingPosition?] {
        let coderFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        return try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams1: { [instanceId] in
                keys.map { RecyclerCollectionIdentifier.identifier(instanceId: instanceId, for: $0.exponent) }
            },
            keyParams2: {
                keys.map { BytesCodable(wrappedValue: $0.publicKey) }
            },
            factory: { coderFactory },
            storagePath: MembersPallet.Storage.members(),
            at: atBlockHash
        )
        .asyncExecute()
        .map(\.value)
    }
}
