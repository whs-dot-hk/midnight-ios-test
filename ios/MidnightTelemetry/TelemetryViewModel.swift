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
    private static let blockStallSecsDefaultsKey = "blockStallSecs"
    private static let genesisDefaultsKey = "genesis"

    @Published private(set) var status: ConnectionStatus = .connecting
    @Published private(set) var snapshot: Snapshot = Snapshot(nodes: [], summary: nil, chains: [])
    @Published private(set) var recentAlerts: [DisplayAlert] = []
    @Published var blockStallSecs: Double = TelemetryViewModel.storedBlockStallSecs() {
        didSet {
            guard blockStallSecs != oldValue else { return }
            UserDefaults.standard.set(blockStallSecs, forKey: Self.blockStallSecsDefaultsKey)
            restart()
        }
    }
    /// Genesis hash of the subscribed chain. The selectable set comes from the
    /// feed itself via `snapshot.chains`, so nothing here is hardcoded.
    @Published var genesis: String = TelemetryViewModel.storedGenesis() {
        didSet {
            guard genesis != oldValue else { return }
            UserDefaults.standard.set(genesis, forKey: Self.genesisDefaultsKey)
            restart()
        }
    }

    private var client: TelemetryClient?
    private var delegateBridge: DelegateBridge?
    private let notifications = NotificationManager()

    /// Label for the subscribed chain, once the feed has announced it.
    var currentChainLabel: String? {
        snapshot.chains.first { $0.genesis == genesis }?.label
    }

    private static func storedBlockStallSecs() -> Double {
        let stored = UserDefaults.standard.double(forKey: blockStallSecsDefaultsKey)
        return stored > 0 ? stored : defaultBlockStallSecs()
    }

    private static func storedGenesis() -> String {
        UserDefaults.standard.string(forKey: genesisDefaultsKey) ?? defaultGenesis()
    }

    func start() {
        guard client == nil else { return }
        let client = TelemetryClient(feedUrl: defaultFeedUrl(), genesis: genesis, blockStallSecs: blockStallSecs)
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

    private func restart() {
        guard client != nil else { return }
        stop()
        status = .connecting
        // Drop the previous chain's nodes and alerts so they aren't shown under
        // the newly selected network, but keep the feed's chain list — that is
        // feed-level, not subscription-specific, so the picker never goes empty.
        snapshot = Snapshot(nodes: [], summary: nil, chains: snapshot.chains)
        recentAlerts = []
        start()
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
