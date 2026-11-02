import Foundation
import NotCore
import Observation
import SystemConfiguration

/// A task plus its nesting depth, in display order.
struct TaskRow: Identifiable {
    let task: TaskItem
    let depth: Int
    var id: String { task.id }
}

@MainActor
@Observable
final class AppModel {
    let inboxID = inboxId()
    private(set) var lists: [TaskList] = []
    private(set) var rows: [TaskRow] = []
    private(set) var syncState: SyncState?
    /// `nil` until this Mac is paired with a server.
    private(set) var syncInfo: SyncInfo?
    var errorMessage: String?
    var selectedListID: String? {
        didSet { if oldValue != selectedListID { reloadTasks() } }
    }

    @ObservationIgnored private let store: NotStore
    @ObservationIgnored private var observer: ModelObserver?

    /// The computer name from System Settings, e.g. "Vitaly's MacBook Air".
    static var defaultDeviceName: String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "Mac"
    }

    init() {
        let dir = URL.applicationSupportDirectory.appending(path: "dev.lysen.not", directoryHint: .isDirectory)
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            store = try NotStore(path: dir.appending(path: "not.db").path)
        } catch {
            fatalError("Cannot open the local database in \(dir.path): \(error)")
        }
        let observer = ModelObserver(model: self)
        store.setObserver(observer: observer)
        self.observer = observer
        selectedListID = inboxID
        reload()

        let defaults = UserDefaults.standard
        // Before pairing, the shared server token was kept here in plain text.
        defaults.removeObject(forKey: "token")
        defaults.removeObject(forKey: "serverURL")

        syncInfo = run { try store.syncInfo() } ?? nil
        run { try store.startSync() }
        // Dev runs: `-serverURL http://127.0.0.1:8080 -pairingCode XXXXX-XXXXX` pairs on launch.
        if syncInfo == nil,
           let url = defaults.string(forKey: "serverURL"),
           let code = defaults.string(forKey: "pairingCode") {
            Task { await pair(serverURL: url, code: code, deviceName: Self.defaultDeviceName) }
        }
    }

    var selectedList: TaskList? { lists.first { $0.id == selectedListID } }
    var inbox: TaskList? { lists.first { $0.id == inboxID } }
    var userLists: [TaskList] { lists.filter { $0.id != inboxID } }

    // MARK: Sync and pairing

    func syncNow() { store.syncNow() }

    fileprivate func didChangeSyncState(_ state: SyncState) { syncState = state }

    @discardableResult
    func pair(serverURL: String, code: String, deviceName: String) async -> Bool {
        let store = store
        guard let info = await runAsync({ try await store.pair(serverUrl: serverURL, code: code, deviceName: deviceName) })
        else { return false }
        syncInfo = info
        return true
    }

    func unpair() async {
        let store = store
        await runAsync { try await store.unpair() }
        syncInfo = nil
        syncState = nil
    }

    func createPairingCode() async -> PairingCode? {
        let store = store
        return await runAsync { try await store.createPairingCode() }
    }

    func devices() async -> [Device] {
        let store = store
        return await runAsync { try await store.devices() } ?? []
    }

    func revoke(_ device: Device) async {
        guard !device.isCurrent else { return await unpair() }
        let store = store
        await runAsync { try await store.revokeDevice(id: device.id) }
    }

    // MARK: Commands

    func addList(_ title: String) {
        if let id = run({ try store.createList(title: title) }) { selectedListID = id }
    }

    func addTask(_ title: String) {
        run { _ = try store.addTask(parentId: selectedListID ?? inboxID, title: title) }
    }

    func toggle(_ task: TaskItem) {
        run { try store.setCompleted(id: task.id, completed: task.completedAt == nil) }
    }

    func delete(id: String) {
        run { try store.deleteItem(id: id) }
    }

    /// `not://add?title=Milk[&list=Groceries]` — the list is matched by title, else the Inbox.
    func handle(url: URL) {
        guard url.scheme == "not", url.host() == "add",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let title = items.first(where: { $0.name == "title" })?.value
        else { return }
        let listName = items.first { $0.name == "list" }?.value ?? ""
        let target = lists.first { $0.title.caseInsensitiveCompare(listName) == .orderedSame }?.id ?? inboxID
        run { _ = try store.addTask(parentId: target, title: title) }
    }

    // MARK: Reading

    fileprivate func reload() {
        guard let lists = run({ try store.lists() }) else { return }
        self.lists = lists
        if let selected = selectedListID, lists.contains(where: { $0.id == selected }) {
            reloadTasks()
        } else {
            selectedListID = inboxID
        }
    }

    private func reloadTasks() {
        guard let list = selectedListID, let tasks = run({ try store.tasks(listId: list) }) else {
            rows = []
            return
        }
        rows = Self.flatten(tasks)
    }

    /// Depth-first order: each task followed by its subtasks.
    private static func flatten(_ tasks: [TaskItem]) -> [TaskRow] {
        let children = Dictionary(grouping: tasks, by: \.parentId)
        var rows: [TaskRow] = []
        func visit(_ parent: String?, depth: Int) {
            for task in children[parent] ?? [] {
                rows.append(TaskRow(task: task, depth: depth))
                visit(task.id, depth: depth + 1)
            }
        }
        visit(nil, depth: 0)
        return rows
    }

    @discardableResult
    private func run<T>(_ body: () throws -> T) -> T? {
        do {
            return try body()
        } catch {
            errorMessage = Self.describe(error)
            return nil
        }
    }

    @discardableResult
    private func runAsync<T: Sendable>(_ body: @Sendable () async throws -> T) async -> T? {
        do {
            return try await body()
        } catch {
            errorMessage = Self.describe(error)
            return nil
        }
    }

    private static func describe(_ error: Error) -> String {
        switch error as? NotError {
        case let .NotFound(detail), let .Invalid(detail), let .Internal(detail): detail
        case nil: error.localizedDescription
        }
    }
}

/// Bridges Rust callbacks (background threads) onto the main actor.
private final class ModelObserver: StoreObserver, @unchecked Sendable {
    private weak var model: AppModel?

    init(model: AppModel) { self.model = model }

    func onChange() {
        Task { @MainActor [weak self] in self?.model?.reload() }
    }

    func onSyncState(state: SyncState) {
        Task { @MainActor [weak self] in self?.model?.didChangeSyncState(state) }
    }
}
