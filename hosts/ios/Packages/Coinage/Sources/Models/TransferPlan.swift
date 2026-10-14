import Foundation

/// A transfer plan produced by the TransferPlanFactory: the strategy to execute.
///
/// Allocation, registration, and memo building happen inside the strategy's `prepare`, so the plan
/// carries nothing more than which strategy to run.
struct TransferPlan {
    /// The strategy to execute.
    let strategy: TransferStrategy
}
