import Foundation

public typealias AnyGraphEdgeProtocol = any GraphEdgeProtocol

public protocol GraphWeightableEdgeProtocol: GraphEdgeProtocol {
    func addingWeight(to currentWeight: Int, predecessor edge: AnyGraphEdgeProtocol?) -> Int
}
