import Foundation

/// `AlertEvent.id` is a stable condition key (e.g. "block-stall"), reused
/// across an alert's fire/escalate/resolve lifecycle so NotificationManager
/// can replace the right system notification. That makes it unsuitable as a
/// SwiftUI list identity for `recentAlerts`, which keeps history: the same
/// condition can appear more than once. Wrap each occurrence with its own id.
struct DisplayAlert: Identifiable {
    let id = UUID()
    let event: AlertEvent
}

@MainActor
final class TelemetryViewModel: ObservableObject {
    @Published private(set) var status: ConnectionStatus = .connecting
    @Published private(set) var snapshot: Snapshot = Snapshot(nodes: [], summary: nil)
    @Published private(set) var recentAlerts: [DisplayAlert] = []

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
        recentAlerts.insert(DisplayAlert(event: alert), at: 0)
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
