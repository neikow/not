import AppKit
import EventKit
import SwiftUI

/// One occurrence of a calendar event, shown read only beside tasks.
struct CalendarEvent: Identifiable, Hashable {
    let id: String
    let title: String
    /// `YYYY-MM-DD`, local.
    let day: String
    /// Nil for all-day events.
    let start: Date?
    let end: Date?
    let color: Color
    /// For "show in Calendar".
    let eventID: String?
}

/// Calendar events in Today and Upcoming, read only: this Mac's calendars (EventKit), the ones
/// picked in Settings → Calendar. Off until switched on there. Mirrors CalendarEvents.kt.
@MainActor
@Observable
final class CalendarOverlay {
    static let enabledKey = "calendarOverlay"
    static let hiddenKey = "calendarHidden"

    /// Events from today through the next 7 days: all-day ones first each day, then by time.
    private(set) var events: [CalendarEvent] = []
    private(set) var access: EKAuthorizationStatus = EKEventStore.authorizationStatus(for: .event)
    var enabled: Bool = UserDefaults.standard.bool(forKey: CalendarOverlay.enabledKey) {
        didSet {
            UserDefaults.standard.set(enabled, forKey: Self.enabledKey)
            if enabled { Task { await requestAccess() } }
            reload()
        }
    }
    /// Calendars left out, by identifier (new calendars show by default).
    var hidden: Set<String> = Set(UserDefaults.standard.stringArray(forKey: CalendarOverlay.hiddenKey) ?? []) {
        didSet {
            UserDefaults.standard.set(Array(hidden), forKey: Self.hiddenKey)
            reload()
        }
    }

    @ObservationIgnored private let store = EKEventStore()
    @ObservationIgnored private var observer: NSObjectProtocol?

    var permitted: Bool { access == .fullAccess }

    init() {
        observer = NotificationCenter.default.addObserver(forName: .EKEventStoreChanged, object: store, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }
        reload()
    }

    func requestAccess() async {
        if access == .notDetermined {
            _ = try? await store.requestFullAccessToEvents()
        }
        access = EKEventStore.authorizationStatus(for: .event)
        reload()
    }

    /// This Mac's event calendars, by account, for Settings.
    var calendars: [EKCalendar] {
        guard permitted else { return [] }
        return store.calendars(for: .event).sorted { ($0.source.title, $0.title) < ($1.source.title, $1.title) }
    }

    func reload() {
        access = EKEventStore.authorizationStatus(for: .event)
        guard enabled, permitted else {
            events = []
            return
        }
        let calendars = store.calendars(for: .event).filter { !hidden.contains($0.calendarIdentifier) }
        guard !calendars.isEmpty else {
            events = []
            return
        }
        let cal = Calendar.current
        let from = cal.startOfDay(for: .now)
        guard let to = cal.date(byAdding: .day, value: 7, to: from) else { return }
        let found = store.events(matching: store.predicateForEvents(withStart: from, end: to, calendars: calendars))
        var out: [CalendarEvent] = []
        for event in found {
            // A multi-day event shows on each of its days in the window.
            var day = max(cal.startOfDay(for: event.startDate), from)
            let last = event.endDate.addingTimeInterval(-1)
            while day < to, day <= last {
                let allDay = event.isAllDay || (event.startDate <= day && event.endDate >= cal.date(byAdding: .day, value: 1, to: day)!)
                out.append(CalendarEvent(
                    id: "\(event.eventIdentifier ?? UUID().uuidString)-\(AppModel.dayString(day))",
                    title: event.title ?? "",
                    day: AppModel.dayString(day),
                    start: allDay ? nil : max(event.startDate, day),
                    end: allDay ? nil : event.endDate,
                    color: Color(nsColor: event.calendar.color ?? .systemGray),
                    eventID: event.eventIdentifier
                ))
                day = cal.date(byAdding: .day, value: 1, to: day)!
            }
        }
        events = out.sorted { a, b in
            if a.day != b.day { return a.day < b.day }
            if (a.start == nil) != (b.start == nil) { return a.start == nil }
            return (a.start ?? .distantPast, a.title) < (b.start ?? .distantPast, b.title)
        }
    }

    func on(_ day: String) -> [CalendarEvent] { events.filter { $0.day == day } }

    /// Opens Calendar (it has no public link to one event).
    func show(_ event: CalendarEvent) {
        NSWorkspace.shared.open(URL(filePath: "/System/Applications/Calendar.app"))
    }
}

/// "9:00–10:00", "All day".
func eventTimeText(_ event: CalendarEvent) -> String {
    guard let start = event.start else { return "All day" }
    let s = start.formatted(date: .omitted, time: .shortened)
    guard let end = event.end, Calendar.current.isDate(end, inSameDayAs: start) else { return s }
    return "\(s)–\(end.formatted(date: .omitted, time: .shortened))"
}

/// An event among tasks: its calendar's color, the time and title. Read only; double-click opens
/// Calendar.
struct EventRow: View {
    let event: CalendarEvent
    let overlay: CalendarOverlay

    var body: some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 2)
                .fill(event.color)
                .frame(width: 4, height: 26)
            VStack(alignment: .leading, spacing: 1) {
                Text(event.title)
                Text(eventTimeText(event)).font(.caption).foregroundStyle(.tertiary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { overlay.show(event) }
        .contextMenu { Button("Show in Calendar") { overlay.show(event) } }
        .help("From Calendar")
    }
}

struct CalendarSettings: View {
    @Bindable var overlay: CalendarOverlay

    var body: some View {
        Form {
            Section {
                Toggle("Show calendar events in Today and Upcoming", isOn: $overlay.enabled)
            } footer: {
                Text("Read only, from this Mac's calendars. On this Mac only.").foregroundStyle(.secondary)
            }
            if overlay.enabled {
                if overlay.access == .denied || overlay.access == .restricted {
                    Section {
                        Text("not can't read your calendars. Allow it in System Settings → Privacy & Security → Calendars.")
                            .foregroundStyle(.secondary)
                        Button("Open Privacy Settings") {
                            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars") {
                                NSWorkspace.shared.open(url)
                            }
                        }
                    }
                } else if overlay.permitted {
                    Section("Calendars") {
                        ForEach(overlay.calendars, id: \.calendarIdentifier) { calendar in
                            Toggle(isOn: Binding(
                                get: { !overlay.hidden.contains(calendar.calendarIdentifier) },
                                set: { shown in
                                    if shown { overlay.hidden.remove(calendar.calendarIdentifier) } else { overlay.hidden.insert(calendar.calendarIdentifier) }
                                }
                            )) {
                                HStack {
                                    Circle().fill(Color(nsColor: calendar.color ?? .systemGray)).frame(width: 9, height: 9)
                                    Text(calendar.title)
                                    Text(calendar.source.title).foregroundStyle(.tertiary)
                                }
                            }
                        }
                    }
                }
            }
        }
        .task { overlay.reload() }
    }
}
