import Foundation
import NotCore
import Observation

/// A task plus its nesting depth, in display order.
struct TaskRow: Identifiable {
    let task: TaskItem
    let depth: Int
    var id: String { task.id }
}

@MainActor
@Observable
final class AppModel {
    private(set) var lists: [TaskList] = []
    private(set) var rows: [TaskRow] = []
    private(set) var syncState: SyncState?
    var errorMessage: String?
    var selectedListID: String? {
        didSet { if oldValue != selectedListID { reloadTasks() } }
    }

    @ObservationIgnored private let store: NotStore
    @ObservationIgnored private var observer: ModelObserver?

    static let serverURLKey = "serverURL"
    static let tokenKey = "token"

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
        reload()
        // UserDefaults also reads `-serverURL … -token …` launch arguments, handy for dev runs.
        let defaults = UserDefaults.standard
        connect(serverURL: defaults.string(forKey: Self.serverURLKey) ?? "",
                token: defaults.string(forKey: Self.tokenKey) ?? "")
    }

    var deviceID: String { store.deviceId() }

    var selectedList: TaskList? { lists.first { $0.id == selectedListID } }

    // MARK: Sync

    func connect(serverURL: String, token: String) {
        let url = serverURL.trimmingCharacters(in: .whitespaces)
        let token = token.trimmingCharacters(in: .whitespaces)
        guard !url.isEmpty, !token.isEmpty else {
            store.stopSync()
            syncState = nil
            return
        }
        store.startSync(serverUrl: url, token: token)
    }

    func syncNow() { store.syncNow() }

    fileprivate func didChangeSyncState(_ state: SyncState) { syncState = state }

    // MARK: Commands

    func addList(_ title: String) {
        if let id = run({ try store.createList(title: title) }) { selectedListID = id }
    }

    func addTask(_ title: String) {
        guard let list = selectedListID ?? ensureInbox() else { return }
        run { _ = try store.addTask(parentId: list, title: title) }
    }

    func toggle(_ task: TaskItem) {
        run { try store.setCompleted(id: task.id, completed: task.completedAt == nil) }
    }

    func delete(id: String) {
        run { try store.deleteItem(id: id) }
    }

    /// `not://add?title=Milk[&list=Groceries]` — the list is matched by title, else the selected one.
    func handle(url: URL) {
        guard url.scheme == "not", url.host() == "add",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let title = items.first(where: { $0.name == "title" })?.value
        else { return }
        let listName = items.first { $0.name == "list" }?.value
        let target = lists.first { $0.title.caseInsensitiveCompare(listName ?? "") == .orderedSame }?.id
            ?? selectedListID ?? ensureInbox()
        guard let target else { return }
        run { _ = try store.addTask(parentId: target, title: title) }
    }

    // MARK: Reading

    fileprivate func reload() {
        guard let lists = run({ try store.lists() }) else { return }
        self.lists = lists
        if let selected = selectedListID, lists.contains(where: { $0.id == selected }) {
            reloadTasks()
        } else {
            selectedListID = lists.first?.id
            if selectedListID == nil { rows = [] }
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

    /// Creates a list when there is none yet, so quick-add always has somewhere to go.
    /// TODO: a real Inbox must be a well-known node, or two fresh devices each create one.
    private func ensureInbox() -> String? {
        lists.first?.id ?? run { try store.createList(title: "Inbox") }
    }

    @discardableResult
    private func run<T>(_ body: () throws -> T) -> T? {
        do {
            return try body()
        } catch {
            errorMessage = "\(error)"
            return nil
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
