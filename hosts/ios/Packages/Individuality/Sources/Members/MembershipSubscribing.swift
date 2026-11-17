import AsyncExtensions
import Foundation
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import SubstrateStorageSubscription

/// The chain readings a membership wait is built from: the member's `Members` entry, the
/// `RingKeysStatus` of a ring, and the ring page size the two are compared with.
public protocol MembershipSubscribing: Sendable {
    func memberPositions(of input: MembershipStatusInput) -> AnyAsyncSequence<MembersPallet.RingPosition?>

    func ringKeysStatuses(
        collection: MembersPallet.CollectionIdentifier,
        ringIndex: MembersPallet.RingIndex
    ) -> AnyAsyncSequence<MembersPallet.RingKeysStatus?>

    func ringKeysPageSize() async throws -> Int
}

public final class ChainMembershipSubscriber: MembershipSubscribing, @unchecked Sendable {
    private let connection: JSONRPCEngine
    private let runtimeCodingService: RuntimeCodingServiceProtocol

    public init(connection: JSONRPCEngine, runtimeCodingService: RuntimeCodingServiceProtocol) {
        self.connection = connection
        self.runtimeCodingService = runtimeCodingService
    }

    public func memberPositions(of input: MembershipStatusInput) -> AnyAsyncSequence<MembersPallet.RingPosition?> {
        let request = DoubleMapSubscriptionRequest(
            storagePath: MembersPallet.Storage.members(),
            localKey: "",
            keyParamClosure: {
                (
                    BytesCodable(wrappedValue: input.collection),
                    BytesCodable(wrappedValue: input.memberKey)
                )
            }
        )

        return subscribe(request)
    }

    public func ringKeysStatuses(
        collection: MembersPallet.CollectionIdentifier,
        ringIndex: MembersPallet.RingIndex
    ) -> AnyAsyncSequence<MembersPallet.RingKeysStatus?> {
        let request = DoubleMapSubscriptionRequest(
            storagePath: MembersPallet.Storage.ringKeysStatus(),
            localKey: "",
            keyParamClosure: {
                (
                    BytesCodable(wrappedValue: collection),
                    StringCodable(wrappedValue: ringIndex)
                )
            }
        )

        return subscribe(request)
    }

    public func ringKeysPageSize() async throws -> Int {
        try await runtimeCodingService.fetchRingKeysPageSize()
    }
}

private extension ChainMembershipSubscriber {
    func subscribe<Value: Decodable>(
        _ request: DoubleMapSubscriptionRequest<some Encodable, some Encodable>
    ) -> AnyAsyncSequence<Value?> {
        let stream: AnyAsyncSequence<BatchStorageSubscriptionSingleResult<Value?>>
        stream = CallbackBatchStorageSubscription.asyncStream(
            requests: [BatchStorageSubscriptionRequest(innerRequest: request, mappingKey: nil)],
            connection: connection,
            runtimeService: runtimeCodingService,
            logger: nil
        )

        return stream.map(\.value).eraseToAnyAsyncSequence()
    }
}
