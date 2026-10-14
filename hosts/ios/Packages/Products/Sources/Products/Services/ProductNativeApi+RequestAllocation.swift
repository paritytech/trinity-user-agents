import Foundation

struct RequestResourceAllocationParams: Decodable {
    let resources: [AllocatableResource]
}

struct RequestResourceAllocationResult: Encodable {
    let outcomes: [AllocationOutcome]
}
