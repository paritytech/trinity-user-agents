import BigInt
import Foundation
import SubstrateSdk
import Testing
@testable import Revive

struct ReviveContractApiTests {
    private static let chainId = "asset-hub"
    private static let contract = EvmAddress(repeating: 0xC0, count: EvmAddressFormat.size)
    private static let input = Data([0x01, 0x02, 0x03])
    private static let readOnlyOrigin = Data(repeating: 0xAA, count: 32)
    private static let signer = Data(repeating: 0xBB, count: 32)

    private func api(_ caller: StubReviveContractCalling) -> ReviveContractApi {
        ReviveContractApi(chainId: Self.chainId, caller: caller, readOnlyOrigin: Self.readOnlyOrigin)
    }

    @Test("a read runs on the bound chain as the read-only origin at the requested block")
    func readOnlyCall() async throws {
        let caller = StubReviveContractCalling(result: .success(.completed(output: Data([0x09]))))
        let blockHash = Data(repeating: 0x11, count: 32)

        let output = try await api(caller).callReadOnly(contract: Self.contract, input: Self.input, at: blockHash)

        #expect(output == Data([0x09]))
        #expect(caller.calls == [
            .init(
                chainId: Self.chainId,
                origin: Self.readOnlyOrigin,
                contract: Self.contract,
                input: Self.input,
                blockHash: blockHash.toHex(includePrefix: true)
            )
        ])
    }

    @Test("a read without a block runs at the latest one")
    func readOnlyCallAtLatest() async throws {
        let caller = StubReviveContractCalling(result: .success(.completed(output: Data())))

        _ = try await api(caller).callReadOnly(contract: Self.contract, input: Self.input, at: nil)

        #expect(caller.calls.first?.blockHash == nil)
    }

    @Test("a reverted read fails with the reason the contract returned")
    func readOnlyRevert() async throws {
        let reason = Data([0xDE, 0xAD])
        let caller = StubReviveContractCalling(result: .success(.completed(output: reason, reverted: true)))

        await #expect(throws: ReviveContractRevertedError(data: reason)) {
            try await api(caller).callReadOnly(contract: Self.contract, input: Self.input, at: nil)
        }
    }

    @Test("a call the pallet rejected fails with the dispatch error, not as a revert")
    func dispatchError() async throws {
        let caller = StubReviveContractCalling(result: .success(.dispatchFailed(.stringValue("BadOrigin"))))

        await #expect(throws: ReviveContractError.self) {
            try await api(caller).callReadOnly(contract: Self.contract, input: Self.input, at: nil)
        }
    }

    @Test("a dry run is simulated as the signer and reports the limits with a charge as the deposit")
    func dryRun() async throws {
        let weight = Substrate.WeightV2(refTime: 1_000, proofSize: 2_000)
        let caller = StubReviveContractCalling(
            result: .success(.completed(output: Data([0x07]), weightRequired: weight, storageDeposit: .charge(413)))
        )

        let dryRun = try await api(caller).dryRun(origin: Self.signer, contract: Self.contract, input: Self.input)

        #expect(dryRun == ReviveDryRun(data: Data([0x07]), weightRequired: weight, storageDeposit: 413))
        #expect(caller.calls.first?.origin == Self.signer)
        #expect(caller.calls.first?.blockHash == nil)
    }

    @Test("a dry run that would only refund declares no deposit")
    func dryRunRefund() async throws {
        let caller = StubReviveContractCalling(result: .success(.completed(output: Data(), storageDeposit: .refund(9))))

        let dryRun = try await api(caller).dryRun(origin: Self.signer, contract: Self.contract, input: Self.input)

        #expect(dryRun.storageDeposit == .zero)
    }

    @Test("a reverted dry run fails like a reverted read")
    func dryRunRevert() async throws {
        let caller = StubReviveContractCalling(result: .success(.completed(output: Data([0x01]), reverted: true)))

        await #expect(throws: ReviveContractRevertedError(data: Data([0x01]))) {
            try await api(caller).dryRun(origin: Self.signer, contract: Self.contract, input: Self.input)
        }
    }

    @Test("the mapping check is asked on the bound chain")
    func mapping() async throws {
        let caller = StubReviveContractCalling(result: .success(.completed(output: Data())), mapped: false)

        let mapped = try await api(caller).isAccountMapped(Self.signer)

        #expect(mapped == false)
        #expect(caller.mappingChecks == [.init(chainId: Self.chainId, account: Self.signer)])
    }
}
