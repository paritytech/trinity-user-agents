import Foundation
@testable import Products

final class MockOSPermissionAsker: OSPermissionAsking, @unchecked Sendable {
    var checkResult: OSPermissionStatus = .notDetermined
    var requestResult: Bool = true

    private(set) var checkCalls: [DeviceCapabilityType] = []
    private(set) var requestCalls: [DeviceCapabilityType] = []

    func checkPermission(for capability: DeviceCapabilityType) async -> OSPermissionStatus {
        checkCalls.append(capability)
        return checkResult
    }

    func requestPermission(for capability: DeviceCapabilityType) async -> Bool {
        requestCalls.append(capability)
        return requestResult
    }
}
