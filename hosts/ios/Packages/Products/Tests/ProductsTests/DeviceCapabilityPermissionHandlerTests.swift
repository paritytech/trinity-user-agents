import Foundation
import Testing
@testable import Products

@Suite("DeviceCapabilityPermissionHandler Tests")
struct DeviceCapabilityPermissionHandlerTests {
    private let productId = "test-product"
    private let capability = DeviceCapabilityType.camera

    private func makeSUT(
        osStatus: OSPermissionStatus = .notDetermined,
        osRequestResult: Bool = true,
        promptDecision: PermissionDecision = .allowAlways
    ) -> (
        handler: DeviceCapabilityPermissionHandler,
        repository: MockProductPermissionRepository,
        requester: MockProductPermissionRequester,
        osAsker: MockOSPermissionAsker
    ) {
        let repository = MockProductPermissionRepository()
        let requester = MockProductPermissionRequester()
        requester.decision = promptDecision
        let osAsker = MockOSPermissionAsker()
        osAsker.checkResult = osStatus
        osAsker.requestResult = osRequestResult
        let handler = DeviceCapabilityPermissionHandler(
            repository: repository,
            requester: requester,
            osAsker: osAsker
        )
        return (handler, repository, requester, osAsker)
    }

    // MARK: - isGranted

    @Test("isGranted returns false when OS permission not granted")
    func isGrantedOsNotGranted() async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .notDetermined)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(!result)
    }

    @Test("isGranted returns false when OS denied")
    func isGrantedOsDenied() async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .denied)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(!result)
    }

    @Test("isGranted returns false when OS allowed but app not granted")
    func isGrantedOsAllowedAppNotGranted() async throws {
        let (handler, _, _, _) = makeSUT(osStatus: .allowed)

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(!result)
    }

    @Test("isGranted returns true when both OS and app allowed")
    func isGrantedBothAllowed() async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .allowed)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .allowedAlways
        )

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(result)
    }

    @Test("isGranted returns true when OS allowed and app allowedOnce")
    func isGrantedOsAllowedAppOnce() async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .allowed)
        repository.grantOneTime(
            productId: productId,
            permission: .deviceCapability(capability)
        )

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(result)
    }

    @Test("isGranted returns false when OS allowed but app denied")
    func isGrantedOsAllowedAppDenied() async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .allowed)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .denied
        )

        let result = try await handler.isGranted(
            productId: productId,
            capability: capability
        )

        #expect(!result)
    }

    // MARK: - request (OS denied)

    @Test("request returns false immediately when OS denied")
    func requestOsDenied() async throws {
        let (handler, _, requester, _) = makeSUT(osStatus: .denied)

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(!result)
        #expect(requester.promptCalls.isEmpty)
    }

    // MARK: - request (already allowed at app level)

    @Test("request skips app prompt when already allowedAlways, requests OS if needed")
    func requestAlreadyAllowedOsNotDetermined() async throws {
        let (handler, repository, requester, osAsker) = makeSUT(
            osStatus: .notDetermined,
            osRequestResult: true
        )
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .allowedAlways
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
        #expect(osAsker.requestCalls.count == 1)
    }

    @Test("request skips both prompts when already allowed and OS allowed")
    func requestAlreadyAllowedOsAllowed() async throws {
        let (handler, repository, requester, osAsker) = makeSUT(osStatus: .allowed)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .allowedAlways
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(result)
        #expect(requester.promptCalls.isEmpty)
        #expect(osAsker.requestCalls.isEmpty)
    }

    // MARK: - request (app denied)

    @Test("request returns false when app permission denied")
    func requestAppDenied() async throws {
        let (handler, repository, requester, _) = makeSUT(osStatus: .notDetermined)
        repository.stubState(
            productId: productId,
            permission: .deviceCapability(capability),
            state: .denied
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(!result)
        #expect(requester.promptCalls.isEmpty)
    }

    // MARK: - request (notDetermined - prompt flows)

    @Test("request prompts app then OS when both notDetermined, allowAlways")
    func requestPromptAllowAlwaysThenOs() async throws {
        let (handler, repository, requester, osAsker) = makeSUT(
            osStatus: .notDetermined,
            osRequestResult: true,
            promptDecision: .allowAlways
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantCalls.count == 1)
        #expect(osAsker.requestCalls.count == 1)
    }

    @Test("request prompts app then OS when notDetermined, allowOnce")
    func requestPromptAllowOnceThenOs() async throws {
        let (handler, repository, requester, osAsker) = makeSUT(
            osStatus: .notDetermined,
            osRequestResult: true,
            promptDecision: .allowOnce
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.grantOneTimeCalls.count == 1)
        #expect(repository.grantCalls.isEmpty)
        #expect(osAsker.requestCalls.count == 1)
    }

    @Test("request prompts app, user denies, does not request OS")
    func requestPromptDenySkipsOs() async throws {
        let (handler, repository, requester, osAsker) = makeSUT(
            osStatus: .notDetermined,
            promptDecision: .deny
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(!result)
        #expect(requester.promptCalls.count == 1)
        #expect(repository.denyCalls.count == 1)
        #expect(osAsker.requestCalls.isEmpty)
    }

    @Test("request prompts app allowAlways, OS already allowed, skips OS request")
    func requestPromptAllowAlwaysOsAlreadyAllowed() async throws {
        let (handler, _, requester, osAsker) = makeSUT(
            osStatus: .allowed,
            promptDecision: .allowAlways
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(result)
        #expect(requester.promptCalls.count == 1)
        #expect(osAsker.requestCalls.isEmpty)
    }

    @Test("request returns false when OS request denied after app allow")
    func requestOsRequestDenied() async throws {
        let (handler, _, _, _) = makeSUT(
            osStatus: .notDetermined,
            osRequestResult: false,
            promptDecision: .allowAlways
        )

        let result = try await handler.request(
            productId: productId,
            capability: capability
        )

        #expect(!result)
    }

    @Test(arguments: [PermissionDecision.allowOnce, .allowAlways, .deny])
    func requestDecisionPreservesConsentWithoutDuplicatingGrants(decision: PermissionDecision) async throws {
        let (handler, repository, _, _) = makeSUT(osStatus: .allowed, promptDecision: decision)
        let result = try await handler.requestDecision(productId: productId, capability: capability)
        #expect(result == decision)
        #expect(try await repository.getPermissionState(
            productId: productId, permission: .deviceCapability(capability)
        ) == .notDetermined)
    }

    @Test
    func requestDecisionDoesNotReuseOrConsumeALegacyOneTimeGrant() async throws {
        let (handler, repository, requester, _) = makeSUT(osStatus: .allowed, promptDecision: .deny)
        repository.grantOneTime(productId: productId, permission: .deviceCapability(capability))
        let result = try await handler.requestDecision(productId: productId, capability: capability)
        #expect(result == .deny)
        #expect(requester.promptCalls.map(\.permission) == [.deviceCapability(capability)])
        #expect(try await repository.getPermissionState(
            productId: productId, permission: .deviceCapability(capability)
        ) == .allowedOnce)
    }

    @Test
    func requestDecisionDoesNotTurnOsRefusalIntoProductDenial() async throws {
        let (handler, repository, _, _) = makeSUT(osRequestResult: false, promptDecision: .allowAlways)
        await #expect(throws: DevicePermissionRequestError.self) {
            try await handler.requestDecision(productId: productId, capability: capability)
        }
        #expect(try await repository.getPermissionState(
            productId: productId, permission: .deviceCapability(capability)
        ) == .notDetermined)
    }
}
