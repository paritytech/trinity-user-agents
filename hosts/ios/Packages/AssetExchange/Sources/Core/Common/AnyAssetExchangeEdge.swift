import Foundation
import Operation_iOS
import SubstrateSdk

public class AnyAssetExchangeEdge {
    let identifier = UUID()

    private let addingWeight: (Int, AnyGraphEdgeProtocol?) -> Int
    private let fetchOrigin: () -> ChainAssetId
    private let fetchDestination: () -> ChainAssetId
    private let fetchQuote: (Balance, AssetConversion.Direction) -> CompoundOperationWrapper<Balance>
    private let beginOperationClosure: (AssetExchangeAtomicOperationArgs) throws -> AssetExchangeAtomicOperationProtocol
    private let appendToOperationClosure: (
        AssetExchangeAtomicOperationProtocol,
        AssetExchangeAtomicOperationArgs
    ) -> AssetExchangeAtomicOperationProtocol?

    private let shouldIgnoreFeeRequirementClosure: (any AssetExchangableGraphEdge) -> Bool
    private let shouldIgnoreDelayedCallReqClosure: (any AssetExchangableGraphEdge) -> Bool
    private let canPayFeesInIntermedPositionClosure: () -> Bool
    private let requiresKeepAliveOnIntermediatePositionClosure: () -> Bool
    private let typeClosure: () -> AssetExchangeEdgeType

    private let beginMetaOperationClosure: (Balance, Balance) throws -> AssetExchangeMetaOperationProtocol

    private let appendToMetaOperationClosure: (AssetExchangeMetaOperationProtocol, Balance, Balance)
        throws -> AssetExchangeMetaOperationProtocol?

    private let beginOperationPrototypeClosure: () throws -> AssetExchangeOperationPrototypeProtocol

    private let appendToOperationPrototypeClosure: (AssetExchangeOperationPrototypeProtocol) throws
        -> AssetExchangeOperationPrototypeProtocol?

    public init(_ edge: any AssetExchangableGraphEdge) {
        addingWeight = edge.addingWeight
        fetchOrigin = { edge.origin }
        fetchDestination = { edge.destination }
        fetchQuote = edge.quote
        beginOperationClosure = edge.beginOperation
        appendToOperationClosure = edge.appendToOperation
        shouldIgnoreFeeRequirementClosure = edge.shouldIgnoreFeeRequirement
        shouldIgnoreDelayedCallReqClosure = edge.shouldIgnoreDelayedCallRequirement
        canPayFeesInIntermedPositionClosure = edge.canPayNonNativeFeesInIntermediatePosition
        requiresKeepAliveOnIntermediatePositionClosure = edge.requiresOriginKeepAliveOnIntermediatePosition
        typeClosure = { edge.type }
        beginMetaOperationClosure = edge.beginMetaOperation
        appendToMetaOperationClosure = edge.appendToMetaOperation
        beginOperationPrototypeClosure = edge.beginOperationPrototype
        appendToOperationPrototypeClosure = edge.appendToOperationPrototype
    }
}

extension AnyAssetExchangeEdge: AssetExchangableGraphEdge {
    public func quote(
        amount: Balance,
        direction: AssetConversion.Direction
    ) -> CompoundOperationWrapper<Balance> {
        fetchQuote(amount, direction)
    }

    public var origin: ChainAssetId { fetchOrigin() }
    public var destination: ChainAssetId { fetchDestination() }
    public var type: AssetExchangeEdgeType { typeClosure() }

    public func addingWeight(
        to currentWeight: Int,
        predecessor edge: AnyGraphEdgeProtocol?
    ) -> Int {
        addingWeight(currentWeight, edge)
    }

    public func beginOperation(
        for args: AssetExchangeAtomicOperationArgs
    ) throws -> AssetExchangeAtomicOperationProtocol {
        try beginOperationClosure(args)
    }

    public func appendToOperation(
        _ currentOperation: AssetExchangeAtomicOperationProtocol,
        args: AssetExchangeAtomicOperationArgs
    ) -> AssetExchangeAtomicOperationProtocol? {
        appendToOperationClosure(currentOperation, args)
    }

    public func shouldIgnoreFeeRequirement(
        after predecessor: any AssetExchangableGraphEdge
    ) -> Bool {
        shouldIgnoreFeeRequirementClosure(predecessor)
    }

    public func shouldIgnoreDelayedCallRequirement(
        after predecessor: any AssetExchangableGraphEdge
    ) -> Bool {
        shouldIgnoreDelayedCallReqClosure(predecessor)
    }

    public func canPayNonNativeFeesInIntermediatePosition() -> Bool {
        canPayFeesInIntermedPositionClosure()
    }

    public func requiresOriginKeepAliveOnIntermediatePosition() -> Bool {
        requiresKeepAliveOnIntermediatePositionClosure()
    }

    public func beginMetaOperation(
        for amountIn: Balance,
        amountOut: Balance
    ) throws -> AssetExchangeMetaOperationProtocol {
        try beginMetaOperationClosure(amountIn, amountOut)
    }

    public func appendToMetaOperation(
        _ currentOperation: AssetExchangeMetaOperationProtocol,
        amountIn: Balance,
        amountOut: Balance
    ) throws -> AssetExchangeMetaOperationProtocol? {
        try appendToMetaOperationClosure(currentOperation, amountIn, amountOut)
    }

    public func beginOperationPrototype() throws -> AssetExchangeOperationPrototypeProtocol {
        try beginOperationPrototypeClosure()
    }

    public func appendToOperationPrototype(
        _ currentPrototype: AssetExchangeOperationPrototypeProtocol
    ) throws -> AssetExchangeOperationPrototypeProtocol? {
        try appendToOperationPrototypeClosure(currentPrototype)
    }
}

extension AnyAssetExchangeEdge: Hashable {
    public static func == (lhs: AnyAssetExchangeEdge, rhs: AnyAssetExchangeEdge) -> Bool {
        lhs.identifier == rhs.identifier
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(identifier)
    }
}
