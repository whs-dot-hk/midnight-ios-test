import Foundation

/// `AlertEvent.id` is a stable condition key (e.g. "block-stall") reused across
/// an alert's fire/resolve lifecycle so NotificationManager can replace the
/// right system notification. Two things follow: it is unsuitable as a SwiftUI
/// list identity for the `recentAlerts` history, since the same condition
/// recurs; and it is only unique *within* one chain, so the chain it came from
/// is carried alongside it.
struct DisplayAlert: Identifiable {
    let id = UUID()
    let event: AlertEvent
    let genesis: String
    let networkLabel: String?
}

/// One monitored chain. The feed allows a single subscription per connection,
/// so each chain gets its own client, and therefore its own state, stall clock
/// and alerts.
struct NetworkMonitor: Identifiable {
    let genesis: String
    /// Known only once the feed announces its chain list.
    var label: String?
    var status: ConnectionStatus = .connecting
    var snapshot: Snapshot = Snapshot(nodes: [], summary: nil, chains: [])

    var id: String { genesis }
}

@MainActor
final class TelemetryViewModel: ObservableObject {
    private static let blockStallSecsDefaultsKey = "blockStallSecs"
    private static let monitorAllNetworksDefaultsKey = "monitorAllNetworks"

    @Published private(set) var monitors: [NetworkMonitor] = []
    @Published private(set) var recentAlerts: [DisplayAlert] = []

    @Published var blockStallSecs: Double = TelemetryViewModel.storedBlockStallSecs() {
        didSet {
            guard blockStallSecs != oldValue else { return }
            UserDefaults.standard.set(blockStallSecs, forKey: Self.blockStallSecsDefaultsKey)
            // Applied live to each running connection. Reconnecting instead
            // would join every telemetry thread on the main thread, freezing
            // the UI for about a second per network on every step of the
            // stepper.
            clients.values.forEach { $0.setBlockStallSecs(secs: blockStallSecs) }
        }
    }

    /// Off by default: mainnet only. On: every chain the feed carries is
    /// monitored on its own connection, and any of them can raise an alert.
    @Published var monitorAllNetworks: Bool =
        UserDefaults.standard.bool(forKey: monitorAllNetworksDefaultsKey)
    {
        didSet {
            guard monitorAllNetworks != oldValue else { return }
            UserDefaults.standard.set(monitorAllNetworks, forKey: Self.monitorAllNetworksDefaultsKey)
            if monitorAllNetworks {
                subscribeToKnownChains()
            } else {
                for genesis in Array(clients.keys) where genesis != defaultGenesis() {
                    unsubscribe(genesis: genesis)
                }
            }
        }
    }

    private var clients: [String: TelemetryClient] = [:]
    private var bridges: [String: DelegateBridge] = [:]
    /// Chains the feed has announced. Every connection announces the full set,
    /// so one is enough to learn about the rest.
    private var knownChains: [ChainOption] = []

    private let notifications = NotificationManager()

    private static func storedBlockStallSecs() -> Double {
        let stored = UserDefaults.standard.double(forKey: blockStallSecsDefaultsKey)
        return stored > 0 ? stored : defaultBlockStallSecs()
    }

    func start() {
        guard clients.isEmpty else { return }
        // Mainnet always runs; the others join once the feed names them.
        subscribe(genesis: defaultGenesis(), label: nil)
    }

    /// `TelemetryClient.stop()` only signals its thread, so this returns at
    /// once; the thread finishes in the background.
    func stop() {
        let running = Array(clients.values)
        clients.removeAll()
        bridges.removeAll()
        monitors.removeAll()
        running.forEach { $0.stop() }
    }

    // MARK: - Connections

    private func subscribe(genesis: String, label: String?) {
        guard clients[genesis] == nil else { return }
        let client = TelemetryClient(
            feedUrl: defaultFeedUrl(), genesis: genesis, blockStallSecs: blockStallSecs)
        let bridge = DelegateBridge(owner: self, genesis: genesis)
        clients[genesis] = client
        bridges[genesis] = bridge
        monitors.append(NetworkMonitor(genesis: genesis, label: label))
        sortMonitors()
        client.start(delegate: bridge)
    }

    private func unsubscribe(genesis: String) {
        let client = clients.removeValue(forKey: genesis)
        bridges[genesis] = nil
        monitors.removeAll { $0.genesis == genesis }
        // Alerts belong to the chain that raised them; keeping them after that
        // chain is dropped would attribute them to nothing.
        recentAlerts.removeAll { $0.genesis == genesis }
        client?.stop()
    }

    private func subscribeToKnownChains() {
        guard monitorAllNetworks else { return }
        for chain in knownChains where clients[chain.genesis] == nil {
            subscribe(genesis: chain.genesis, label: chain.label)
        }
    }

    /// Mainnet first, then alphabetical, so the order stays put as counts move.
    private func sortMonitors() {
        let mainnet = defaultGenesis()
        monitors.sort {
            if ($0.genesis == mainnet) != ($1.genesis == mainnet) {
                return $0.genesis == mainnet
            }
            return ($0.label ?? $0.genesis) < ($1.label ?? $1.genesis)
        }
    }

    // MARK: - Delegate callbacks

    /// A stopped thread is not joined, so callbacks can still arrive for a
    /// subscription already dropped. `clients` is the record of what is still
    /// wanted, and anything else is discarded — otherwise a late alert would
    /// notify about a network no longer being monitored.
    private func isActive(_ genesis: String) -> Bool {
        clients[genesis] != nil
    }

    fileprivate func handleSnapshot(_ snapshot: Snapshot, genesis: String) {
        guard isActive(genesis) else { return }
        if !snapshot.chains.isEmpty {
            knownChains = snapshot.chains
            for i in monitors.indices where monitors[i].label == nil {
                monitors[i].label = snapshot.chains
                    .first { $0.genesis == monitors[i].genesis }?.label
            }
            sortMonitors()
            subscribeToKnownChains()
        }
        if let i = monitors.firstIndex(where: { $0.genesis == genesis }) {
            monitors[i].snapshot = snapshot
        }
    }

    fileprivate func handleAlert(_ alert: AlertEvent, genesis: String) {
        guard isActive(genesis) else { return }
        let label = monitors.first { $0.genesis == genesis }?.label
        recentAlerts.insert(
            DisplayAlert(event: alert, genesis: genesis, networkLabel: label), at: 0)
        recentAlerts = Array(recentAlerts.prefix(20))
        notifications.post(alert: alert, genesis: genesis, networkLabel: label)
    }

    fileprivate func handleStatusChanged(_ status: ConnectionStatus, genesis: String) {
        guard isActive(genesis) else { return }
        if let i = monitors.firstIndex(where: { $0.genesis == genesis }) {
            monitors[i].status = status
        }
    }
}

/// Rust calls these from each connection's own background telemetry thread;
/// every callback hops to the main actor before touching view-model state, and
/// carries the genesis hash identifying which chain it came from.
private final class DelegateBridge: TelemetryDelegate {
    private weak var owner: TelemetryViewModel?
    private let genesis: String

    init(owner: TelemetryViewModel, genesis: String) {
        self.owner = owner
        self.genesis = genesis
    }

    func onSnapshot(snapshot: Snapshot) {
        let owner = self.owner
        let genesis = self.genesis
        Task { @MainActor in owner?.handleSnapshot(snapshot, genesis: genesis) }
    }

    func onAlert(alert: AlertEvent) {
        let owner = self.owner
        let genesis = self.genesis
        Task { @MainActor in owner?.handleAlert(alert, genesis: genesis) }
    }

    func onStatusChanged(status: ConnectionStatus) {
        let owner = self.owner
        let genesis = self.genesis
        Task { @MainActor in owner?.handleStatusChanged(status, genesis: genesis) }
    }
}
