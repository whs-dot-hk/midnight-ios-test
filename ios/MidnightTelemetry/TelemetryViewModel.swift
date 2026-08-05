import Foundation

@MainActor
final class TelemetryViewModel: ObservableObject {
    @Published private(set) var status: ConnectionStatus = .connecting
    @Published private(set) var snapshot: Snapshot = Snapshot(nodes: [], summary: nil)
    @Published private(set) var recentAlerts: [AlertEvent] = []

    private var client: TelemetryClient?
    private var delegateBridge: DelegateBridge?
    private let notifications = NotificationManager()

    func start(networkId: String = defaultNetworkId()) {
        guard client == nil else { return }
        let client = TelemetryClient(feedUrl: defaultFeedUrl(), networkId: networkId)
        let bridge = DelegateBridge(owner: self)
        self.client = client
        self.delegateBridge = bridge
        client.start(delegate: bridge)
    }

    func stop() {
        client?.stop()
        client = nil
        delegateBridge = nil
    }

    fileprivate func handleSnapshot(_ snapshot: Snapshot) {
        self.snapshot = snapshot
    }

    fileprivate func handleAlert(_ alert: AlertEvent) {
        recentAlerts.insert(alert, at: 0)
        recentAlerts = Array(recentAlerts.prefix(20))
        notifications.post(alert: alert)
    }

    fileprivate func handleStatusChanged(_ status: ConnectionStatus) {
        self.status = status
    }
}

/// Rust calls these from its own background telemetry thread; each callback
/// hops to the main actor before touching view-model state.
private final class DelegateBridge: TelemetryDelegate {
    private weak var owner: TelemetryViewModel?

    init(owner: TelemetryViewModel) {
        self.owner = owner
    }

    func onSnapshot(snapshot: Snapshot) {
        Task { @MainActor in owner?.handleSnapshot(snapshot) }
    }

    func onAlert(alert: AlertEvent) {
        Task { @MainActor in owner?.handleAlert(alert) }
    }

    func onStatusChanged(status: ConnectionStatus) {
        Task { @MainActor in owner?.handleStatusChanged(status) }
    }
}
