import BigInt
import Coinage
import Foundation
import Operation_iOS
import Testing

@testable import polkadot_app

extension CoreDataMapperTests {
    @Suite("CoinMapper")
    struct CoinMapperTests {
        private let facade = UserDataStorageTestFacade()
        private var repo: AnyDataProviderRepository<Coin> { facade.makeRepo(mapper: CoinMapper()) }

        private func roundTrip(_ coin: Coin) async throws -> Coin {
            try await repo.saveOperation({ [coin] }, { [] }).asyncExecute()
            return try #require(
                try await repo.fetchOperation(by: { coin.identifier }, options: .init()).asyncExecute()
            )
        }

        @Test("roundTrip preserves the stored identity fields")
        func roundTrip() async throws {
            let original = Coin(
                exponent: 12,
                derivationIndex: 42,
                age: 5,
                isOnchain: true,
                publicKey: Data(repeating: 0x2A, count: 32)
            )
            let result = try await roundTrip(original)

            #expect(result.exponent == original.exponent)
            #expect(result.derivationIndex == original.derivationIndex)
            #expect(result.age == original.age)
            #expect(result.isOnchain == original.isOnchain)
            #expect(result.publicKey == original.publicKey)
        }

        @Test("nil age stored as -1 and restored to nil")
        func nilAgeRestoredToNil() async throws {
            let result = try await roundTrip(
                Coin(exponent: 8, derivationIndex: 100, age: nil, publicKey: Data(repeating: 0x64, count: 32))
            )
            #expect(result.age == nil)
        }

        // Status is derived on read from the durability graph, presence and age — not stored. With
        // no durability entries, only presence/age drive it.

        // Coin status is no longer a stored `Coin.state`; it is derived on read as a `TrackedCoin`
        // from the durability graph, presence and age, and is covered by the derive-on-read tests.

        // MARK: - hops (SCALE-encoded, tagged variants)

        private func key(_ byte: UInt8) -> Data { Data(repeating: byte, count: 32) }

        @Test("hop chain and fungibility survive a round trip")
        func provenanceRoundTrips() async throws {
            let hops: [Hop] = [.transfer(bundleSize: 3), .split(fanout: 4), .transfer(bundleSize: 1)]
            let result = try await roundTrip(
                Coin(
                    exponent: 6,
                    derivationIndex: 500,
                    age: 2,
                    recyclerFungibility: 73,
                    hops: hops,
                    publicKey: key(0x11)
                )
            )

            #expect(result.hops == hops)
            #expect(result.recyclerFungibility == 73)
        }

        @Test("empty hops round-trip as empty, not nil")
        func emptyHops() async throws {
            let result = try await roundTrip(
                Coin(exponent: 4, derivationIndex: 501, age: nil, hops: [], publicKey: key(0x12))
            )
            #expect(result.hops.isEmpty)
        }

        @Test("hop order is preserved")
        func hopOrderPreserved() async throws {
            let hops: [Hop] = [
                .split(fanout: 2),
                .transfer(bundleSize: 7),
                .split(fanout: 9),
                .transfer(bundleSize: 1)
            ]
            let result = try await roundTrip(
                Coin(exponent: 4, derivationIndex: 502, age: nil, hops: hops, publicKey: key(0x13))
            )
            #expect(result.hops == hops)
        }

        @Test("hop payloads survive at UInt8 bounds", arguments: [UInt8.min, 1, UInt8.max])
        func hopPayloadBounds(payload: UInt8) async throws {
            let hops: [Hop] = [.transfer(bundleSize: payload), .split(fanout: payload)]
            let result = try await roundTrip(
                Coin(
                    exponent: 4,
                    derivationIndex: CoinageKeyIndex(installation: .test, item: 600 + UInt64(payload)),
                    age: nil,
                    hops: hops,
                    publicKey: key(0x14)
                )
            )
            #expect(result.hops == hops)
        }

        @Test("a long hop chain round-trips")
        func longHopChain() async throws {
            let hops: [Hop] = (0 ..< 64).map {
                $0.isMultiple(of: 2) ? .transfer(bundleSize: UInt8($0 % 256)) : .split(fanout: UInt8($0 % 256))
            }
            let result = try await roundTrip(
                Coin(exponent: 4, derivationIndex: 700, age: nil, hops: hops, publicKey: key(0x15))
            )
            #expect(result.hops == hops)
        }

        // MARK: - recyclerFungibility

        @Test("recyclerFungibility round-trips", arguments: [UInt8.min, 50, CoinageConstants.fullFungibility])
        func fungibilityRoundTrips(value: UInt8) async throws {
            let result = try await roundTrip(
                Coin(
                    exponent: 4,
                    derivationIndex: CoinageKeyIndex(installation: .test, item: 800 + UInt64(value)),
                    age: nil,
                    recyclerFungibility: value,
                    publicKey: key(0x16)
                )
            )
            #expect(result.recyclerFungibility == value)
        }

        @Test("an unknown fungibility round-trips as nil, not as zero")
        func unknownFungibilityRoundTrips() async throws {
            let result = try await roundTrip(
                Coin(exponent: 4, derivationIndex: 900, age: nil, recyclerFungibility: nil, publicKey: key(0x17))
            )
            #expect(result.recyclerFungibility == nil)
        }
    }
}
