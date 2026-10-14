import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

/// Pinned against ethers v6 encoding the contract's own ABI (`abi/AccountDataStore.json`).
struct AccountDataStoreAbiTests {
    private static let record = "0xb3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e80acfc55b8155"
        + "9e511662b8d79151ac504ca50de705fe6736054d586"
    private static let owner = "0x1111111111111111111111111111111111111111"

    private static let registerCall = "0xe561868d"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "000000000000000000000000000000000000000000000000000000000000003c"
        + "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e"
        + "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000"

    private static let getCall = "0x740204c60000000000000000000000001111111111111111111111111111111111111111"

    private static let returnedList = "0x"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "0000000000000000000000000000000000000000000000000000000000000002"
        + "0000000000000000000000000000000000000000000000000000000000000040"
        + "00000000000000000000000000000000000000000000000000000000000000a0"
        + "000000000000000000000000000000000000000000000000000000000000003c"
        + "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e"
        + "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000"
        + "0000000000000000000000000000000000000000000000000000000000000002"
        + "0102000000000000000000000000000000000000000000000000000000000000"

    private static let returnedEmpty = "0x"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "0000000000000000000000000000000000000000000000000000000000000000"

    @Test("registering encodes the record as dynamic bytes after the selector")
    func encodeRegister() throws {
        let encoded = try AccountDataStoreAbi.encodeRegisterInstallation(record: Data(hexString: Self.record))
        #expect(encoded.toHex(includePrefix: true) == Self.registerCall)
    }

    @Test("the installation list is asked for by address")
    func encodeGet() throws {
        let encoded = try AccountDataStoreAbi.encodeGetInstallations(owner: Data(hexString: Self.owner))
        #expect(encoded.toHex(includePrefix: true) == Self.getCall)
    }

    @Test("a returned list of records decodes in order")
    func decodeList() throws {
        let decoded = try AccountDataStoreAbi.decodeGetInstallations(output: Data(hexString: Self.returnedList))
        #expect(try decoded == [Data(hexString: Self.record), Data([0x01, 0x02])])
    }

    @Test("an empty list decodes to no records")
    func decodeEmpty() throws {
        let decoded = try AccountDataStoreAbi.decodeGetInstallations(output: Data(hexString: Self.returnedEmpty))
        #expect(decoded.isEmpty)
    }

    @Test("an unreadable answer fails instead of passing for an empty list")
    func decodeGarbage() {
        #expect(throws: AccountDataStoreAbiError.self) {
            try AccountDataStoreAbi.decodeGetInstallations(output: Data([0x01, 0x02, 0x03]))
        }
    }
}
