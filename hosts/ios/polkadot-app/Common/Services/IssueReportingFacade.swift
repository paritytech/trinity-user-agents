import IssueMonitoring

enum IssueReportingFacade {
    static let shared: IssueReporting = IssueMonitoringFactory.createReporter()
}
