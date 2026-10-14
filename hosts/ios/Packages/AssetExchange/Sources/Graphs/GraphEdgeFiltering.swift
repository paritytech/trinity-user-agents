import Foundation

public protocol GraphEdgeFiltering {
    associatedtype Edge

    func shouldVisit(edge: Edge, predecessor: Edge?) -> Bool
}

public class AnyGraphEdgeFilter<E> {
    public typealias Edge = E

    private let shouldVisitClosure: (Edge, Edge?) -> Bool

    public init<F: GraphEdgeFiltering>(filter: F) where F.Edge == E {
        shouldVisitClosure = filter.shouldVisit
    }

    public init(closure: @escaping (Edge, Edge?) -> Bool) {
        shouldVisitClosure = closure
    }
}

extension AnyGraphEdgeFilter: GraphEdgeFiltering {
    public func shouldVisit(edge: Edge, predecessor: Edge?) -> Bool {
        shouldVisitClosure(edge, predecessor)
    }
}

public extension AnyGraphEdgeFilter {
    static func allEdges() -> AnyGraphEdgeFilter<E> {
        AnyGraphEdgeFilter { _, _ in true }
    }
}
