import AppKit
import NotCore
import UserNotifications

/// What something sent to not becomes, before it's saved.
struct SharePlan: Equatable {
    let asTask: Bool
    let title: String
    let body: String
}

/// The smart default, as on the phone (`planShare` in ShareActivity.kt): a link or a short single
/// line becomes a task (the link kept in its notes); longer or multi-line text, or anything with
/// pictures, a note.
func planShare(subject: String?, text: String?, images: Int) -> SharePlan {
    let text = (text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let subject = (subject ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let urlPattern = /https?:\/\/\S+/
    let url = text.firstMatch(of: urlPattern).map { String($0.output) }
    let words = text.replacing(urlPattern, with: "")
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .trimmingCharacters(in: CharacterSet(charactersIn: "-–|:"))
        .trimmingCharacters(in: .whitespacesAndNewlines)
    let lines = text.split(separator: "\n").count { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
    let asTask = images == 0 && ((url != nil && words.count <= 140) || (lines <= 1 && text.count <= 140))
    if asTask {
        let title: String
        if !words.isEmpty {
            title = words.split(separator: "\n").first.map { $0.trimmingCharacters(in: .whitespaces) } ?? words
        } else if !subject.isEmpty {
            title = subject
        } else if let url {
            var short = url
            for prefix in ["https://", "http://", "www."] where short.hasPrefix(prefix) { short.removeFirst(prefix.count) }
            while short.hasSuffix("/") { short.removeLast() }
            title = short
        } else {
            title = text
        }
        return SharePlan(asTask: true, title: title, body: url ?? "")
    }
    // A note: the subject (if any) as its first line, then the text as it came.
    let parts = [(!subject.isEmpty && !text.hasPrefix(subject)) ? subject : nil, text.isEmpty ? nil : text].compactMap { $0 }
    return SharePlan(asTask: false, title: "", body: parts.joined(separator: "\n"))
}

/// macOS Services ("Add to not", "New not Note"): text, links and pictures from any app's
/// Services menu (and right-click), saved without leaving that app; a notification says where.
@MainActor
final class ServiceProvider: NSObject {
    private weak var model: AppModel?

    init(model: AppModel) {
        self.model = model
    }

    /// The smart default: a task in the Inbox, or a note.
    @objc func addToNot(_ pboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString?>) {
        save(pboard, forceNote: false)
    }

    /// Always a note.
    @objc func newNote(_ pboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString?>) {
        save(pboard, forceNote: true)
    }

    private func save(_ pboard: NSPasteboard, forceNote: Bool) {
        guard let model else { return }
        let text = pboard.string(forType: .string) ?? pboard.string(forType: .URL)
        let images = Self.images(pboard)
        let plan = planShare(subject: nil, text: text, images: images.count)
        if plan.asTask, !forceNote {
            guard model.addSharedTask(title: plan.title, link: plan.body) != nil else { return }
            Self.confirm("Added to Inbox", plan.title)
        } else {
            let body = forceNote && plan.asTask ? (text ?? "") : plan.body
            guard model.addSharedNote(body: body, images: images) != nil else { return }
            let first = body.split(separator: "\n").first.map(String.init) ?? (images.isEmpty ? "" : "Picture")
            Self.confirm("Saved as a note", first)
        }
    }

    /// Pictures on the pasteboard, as image files' data or image data itself.
    private static func images(_ pboard: NSPasteboard) -> [Data] {
        if let urls = pboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingContentsConformToTypes: ["public.image"]]) as? [URL], !urls.isEmpty {
            return urls.compactMap { try? Data(contentsOf: $0) }
        }
        if let images = pboard.readObjects(forClasses: [NSImage.self]) as? [NSImage] {
            return images.compactMap(\.tiffRepresentation)
        }
        return []
    }

    private static func confirm(_ title: String, _ body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
    }
}
