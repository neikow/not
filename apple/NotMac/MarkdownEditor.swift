import AppKit
import UniformTypeIdentifiers
import NotCore
import SwiftUI

/// A plain-text markdown editor that styles the markdown in place (headings, emphasis, checklists),
/// using the core's `markdownSpans` so it renders like the Android app. Click a checkbox to tick it;
/// Return continues lists; ⌘B / ⌘I wrap the selection.
struct MarkdownEditor: NSViewRepresentable {
    @Binding var text: String
    /// Bumped by the owner to move keyboard focus here.
    var focusRequests = 0
    /// Where "Move to Tasks" in the context menu can send checklist items.
    var taskLists: [TaskList] = []
    /// Moves the checklist items in a UTF-16 range of the text into a list (nil: a new one with
    /// the given title). Without it the menu item doesn't show.
    var onMakeTasks: ((_ range: NSRange, _ listID: String?, _ title: String) -> Void)?
    /// Keeps pasted or dropped image data (downscaled first) and returns its id.
    var onAddImage: ((Data) -> String?)?
    /// An image id to put in at the caret, with a counter so the same picture can go in twice.
    var imageToInsert: (id: String, request: Int)?
    /// Bumped when images arrive from the server, so placeholders redraw.
    var imageVersion = 0
    /// Line indexes of checklist lines linked to tasks: their boxes are drawn in the accent.
    var linkedLines: Set<Int> = []
    /// What "Make tasks" would take from a UTF-16 range of the text (items not linked yet).
    var checklist: ((_ text: String, _ range: NSRange) -> NoteChecklist?)?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        // TextKit 1: drawing boxes needs glyph positions from the layout manager, and asking a
        // TextKit 2 view for it switches it over mid-flight.
        let editor = MarkdownTextView(usingTextLayoutManager: false)
        editor.frame = NSRect(origin: .zero, size: scroll.contentSize)
        editor.autoresizingMask = [.width]
        editor.isVerticallyResizable = true
        editor.isHorizontallyResizable = false
        editor.textContainer?.widthTracksTextView = true
        editor.minSize = .zero
        editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.allowsUndo = true
        editor.drawsBackground = false
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.textContainerInset = NSSize(width: 28, height: 22)
        editor.typingAttributes = MarkdownStyler.base
        editor.registerForDraggedTypes([.fileURL, .png, .tiff])
        editor.string = text
        MarkdownStyler.apply(to: editor)
        scroll.documentView = editor
        context.coordinator.textView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let editor = context.coordinator.textView else { return }
        if editor.string != text {
            let selection = editor.selectedRange()
            editor.string = text
            MarkdownStyler.apply(to: editor)
            let length = (text as NSString).length
            editor.setSelectedRange(NSRange(location: min(selection.location, length), length: 0))
        }
        editor.onAddImage = onAddImage
        if editor.linkedLines != linkedLines { editor.linkedLines = linkedLines }
        if let insert = imageToInsert, context.coordinator.imageRequest != insert.request {
            context.coordinator.imageRequest = insert.request
            editor.insertImageLine(imageMarkup(id: insert.id))
        }
        if context.coordinator.imageVersion != imageVersion {
            context.coordinator.imageVersion = imageVersion
            MarkdownStyler.apply(to: editor)
        }
        if context.coordinator.focusRequests != focusRequests {
            context.coordinator.focusRequests = focusRequests
            DispatchQueue.main.async { editor.window?.makeFirstResponder(editor) }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: MarkdownEditor
        weak var textView: MarkdownTextView?
        var focusRequests: Int
        var imageRequest: Int?
        var imageVersion: Int

        init(_ parent: MarkdownEditor) {
            self.parent = parent
            focusRequests = parent.focusRequests
            imageRequest = parent.imageToInsert?.request
            imageVersion = parent.imageVersion
        }

        /// Nothing is typed into an image's hidden markup: the caret goes after it.
        func textViewDidChangeSelection(_ notification: Notification) {
            guard let textView = notification.object as? MarkdownTextView else { return }
            let selection = textView.selectedRange()
            guard selection.length == 0,
                  let line = textView.atomicLines.first(where: { NSLocationInRange(selection.location, $0) })
            else { return }
            textView.setSelectedRange(NSRange(location: NSMaxRange(line), length: 0))
        }

        /// Backspace into an image takes it out whole; typing right after one starts a new line.
        func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
            guard let view = textView as? MarkdownTextView, let text, !view.editingImages else { return true }
            let ns = view.string as NSString
            if text.isEmpty, range.length > 0,
               let line = view.atomicLines.first(where: { NSIntersectionRange($0, range).length > 0 }),
               NSMaxRange(range) <= NSMaxRange(line) {
                var whole = line
                if whole.location > 0 { whole = NSRange(location: whole.location - 1, length: whole.length + 1) }
                else if NSMaxRange(whole) < ns.length { whole.length += 1 }
                DispatchQueue.main.async { view.editImages { view.replace(whole, with: "") } }
                return false
            }
            if !text.isEmpty, !text.hasPrefix("\n"), range.length == 0,
               view.atomicLines.contains(where: { NSMaxRange($0) == range.location }) {
                DispatchQueue.main.async { view.editImages { view.replace(range, with: "\n" + text) } }
                return false
            }
            return true
        }

        func textDidChange(_ notification: Notification) {
            guard let textView else { return }
            MarkdownStyler.apply(to: textView)
            parent.text = textView.string
        }

        /// "Make N Tasks In ▸ New List “Groceries” / Inbox / lists…" for the checklist items the
        /// selection touches and that aren't linked yet; nil when there are none. The lines stay,
        /// linked to their tasks.
        func makeTasksItem(for textView: NSTextView) -> NSMenuItem? {
            guard let make = parent.onMakeTasks else { return nil }
            let range = textView.selectedRange()
            guard let checklist = parent.checklist?(textView.string, range) else { return nil }
            let items = checklist.count == 1 ? "1 Task" : "\(checklist.count) Tasks"
            let item = NSMenuItem(title: "Make \(items) In", action: nil, keyEquivalent: "")
            let submenu = NSMenu()
            func add(_ title: String, _ image: String, _ run: @escaping () -> Void) {
                let entry = ClosureMenuItem(title: title, run: run)
                entry.image = NSImage(systemSymbolName: image, accessibilityDescription: nil)
                submenu.addItem(entry)
            }
            add("New List “\(checklist.suggestedTitle)”", "plus") { make(range, nil, checklist.suggestedTitle) }
            submenu.addItem(.separator())
            add("Inbox", "tray") { make(range, inboxId(), "Inbox") }
            for list in parent.taskLists where list.id != inboxId() {
                add(list.title, "list.bullet") { make(range, list.id, list.title) }
            }
            item.submenu = submenu
            return item
        }

        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            return (textView as? MarkdownTextView)?.continueList() ?? false
        }
    }
}

final class MarkdownTextView: NSTextView {
    /// Image lines (whole line, without its newline) the editor draws over; set by `MarkdownStyler`.
    var imageLines: [(range: NSRange, id: String)] = [] {
        didSet { needsDisplay = true }
    }
    /// Line indexes of checklist lines linked to tasks.
    var linkedLines: Set<Int> = [] {
        didSet { needsDisplay = true }
    }
    /// Lines edited only whole: images.
    var atomicLines: [NSRange] { imageLines.map(\.range) }
    var onAddImage: ((Data) -> String?)?
    private var styledWidth: CGFloat = 0
    /// Set while the image rules make their own edit, so they don't apply to it again.
    private(set) var editingImages = false

    func editImages(_ edit: () -> Void) {
        editingImages = true
        defer { editingImages = false }
        edit()
    }

    /// Images are sized to the width, so a resize restyles them.
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if !imageLines.isEmpty, abs(newSize.width - styledWidth) > 1 {
            styledWidth = newSize.width
            MarkdownStyler.apply(to: self)
        }
    }

    /// Pasting a picture (a screenshot, a copied photo) adds it as an image line.
    override func paste(_ sender: Any?) {
        let board = NSPasteboard.general
        let hasText = board.string(forType: .string) != nil
        if !hasText, addImages(from: board) { return }
        super.paste(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        if addImages(from: sender.draggingPasteboard) { return true }
        return super.performDragOperation(sender)
    }

    /// Image files or image data on `board`, each as a line of its own at the caret.
    private func addImages(from board: NSPasteboard) -> Bool {
        guard let add = onAddImage else { return false }
        var datas: [Data] = []
        let urls = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingContentsConformToTypes: [UTType.image.identifier]]) as? [URL] ?? []
        datas += urls.compactMap { try? Data(contentsOf: $0) }
        if datas.isEmpty, let data = board.data(forType: .png) ?? board.data(forType: .tiff) {
            datas.append(data)
        }
        let ids = datas.compactMap { NoteImageEncoder.encode($0).flatMap(add) }
        for id in ids { insertImageLine(imageMarkup(id: id)) }
        return !ids.isEmpty
    }

    /// Puts an image on a line of its own at the caret: after the caret's line, or in place of it
    /// when it's empty, with the caret on a fresh line below.
    func insertImageLine(_ markup: String) {
        let ns = string as NSString
        let at = NSMaxRange(selectedRange())
        let line = ns.lineRange(for: NSRange(location: at, length: 0))
        var content = line
        if content.length > 0, ns.character(at: NSMaxRange(content) - 1) == 10 { content.length -= 1 }
        let empty = ns.substring(with: content).trimmingCharacters(in: .whitespaces).isEmpty
        let target = empty ? content : NSRange(location: NSMaxRange(content), length: 0)
        let before = empty ? "" : "\n"
        let after = NSMaxRange(target) < ns.length ? "" : "\n"
        editImages { replace(target, with: before + markup + "\n" + after) }
        setSelectedRange(NSRange(location: target.location + (before + markup + "\n" as NSString).length, length: 0))
    }

    /// Checklist boxes and bullets drawn over their hidden markup; set by `MarkdownStyler`.
    var marks: [ListMark] = [] {
        didSet { needsDisplay = true }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = super.menu(for: event) ?? NSMenu()
        if let item = (delegate as? MarkdownEditor.Coordinator)?.makeTasksItem(for: self) {
            menu.insertItem(item, at: 0)
            menu.insertItem(.separator(), at: 1)
        }
        return menu
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        for case let .box(at, checked) in marks where boxRect(at: at).insetBy(dx: -6, dy: -4).contains(point) {
            replace(NSRange(location: at + 1, length: 1), with: checked ? " " : "x")
            return
        }
        super.mouseDown(with: event)
    }

    override func drawBackground(in rect: NSRect) {
        super.drawBackground(in: rect)
        for line in imageLines {
            guard let box = imageRect(line.range), box.intersects(rect) else { continue }
            NSGraphicsContext.saveGraphicsState()
            let path = NSBezierPath(roundedRect: box, xRadius: 10, yRadius: 10)
            if let image = NoteImageCache.shared.image(line.id) {
                path.addClip()
                image.draw(in: box, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
            } else {
                NSColor.quaternaryLabelColor.withAlphaComponent(0.15).setFill()
                path.fill()
            }
            NSGraphicsContext.restoreGraphicsState()
        }
        let ns = string as NSString
        for mark in marks {
            switch mark {
            case let .box(at, checked):
                let box = boxRect(at: at)
                guard box.intersects(rect) else { continue }
                let line = ns.substring(to: min(at, ns.length)).reduce(0) { $1 == "\n" ? $0 + 1 : $0 }
                drawCheckbox(box, checked: checked, linked: linkedLines.contains(line))
            case let .dot(at):
                let center = markCenter(at: at)
                NSColor.controlAccentColor.setFill()
                NSBezierPath(ovalIn: NSRect(x: center.x - 2.5, y: center.y - 2.5, width: 5, height: 5)).fill()
            }
        }
    }

    /// A box; `linked` (its line is linked to a task) draws an open one in the accent.
    private func drawCheckbox(_ box: NSRect, checked: Bool, linked: Bool = false) {
        let path = NSBezierPath(roundedRect: box, xRadius: 3.5, yRadius: 3.5)
        if checked {
            NSColor.controlAccentColor.setFill()
            path.fill()
            let check = NSBezierPath()
            check.move(to: NSPoint(x: box.minX + box.width * 0.24, y: box.minY + box.height * 0.52))
            check.line(to: NSPoint(x: box.minX + box.width * 0.43, y: box.minY + box.height * 0.70))
            check.line(to: NSPoint(x: box.minX + box.width * 0.77, y: box.minY + box.height * 0.32))
            check.lineWidth = 1.8
            check.lineCapStyle = .round
            check.lineJoinStyle = .round
            NSColor.white.setStroke()
            check.stroke()
        } else {
            let inset = NSBezierPath(roundedRect: box.insetBy(dx: 0.75, dy: 0.75), xRadius: 3, yRadius: 3)
            inset.lineWidth = 1.5
            (linked ? NSColor.controlAccentColor : NSColor.tertiaryLabelColor).setStroke()
            inset.stroke()
        }
    }

    /// Where an image line's picture goes: its line fragment, less the gap, as wide as it fits.
    private func imageRect(_ range: NSRange) -> NSRect? {
        guard let layout = layoutManager, range.location < (string as NSString).length else { return nil }
        let glyph = layout.glyphIndexForCharacter(at: range.location)
        let line = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let origin = textContainerOrigin
        let padding = textContainer?.lineFragmentPadding ?? 0
        let height = line.height - MarkdownStyler.imageGap * 2 - 7 // less lineSpacing + paragraphSpacing
        guard height > 0 else { return nil }
        var width = line.width - padding * 2
        if let image = NoteImageCache.shared.image(imageLines.first { $0.range == range }?.id ?? "") , image.size.height > 0 {
            width = min(width, height * image.size.width / image.size.height)
        }
        return NSRect(x: origin.x + line.minX + padding, y: origin.y + line.minY + MarkdownStyler.imageGap, width: width, height: height)
    }

    /// The square a box is drawn in, centred on its `[ ]`.
    private func boxRect(at: Int) -> NSRect {
        let center = markCenter(at: at)
        let side: CGFloat = 14
        return NSRect(x: center.x - side / 2, y: center.y - side / 2, width: side, height: side)
    }

    /// Where a box or bullet is drawn: half the box markup's width in from [at], centred on the
    /// line's capital letters.
    private func markCenter(at: Int) -> NSPoint {
        guard let layout = layoutManager, at < (string as NSString).length else { return .zero }
        let glyph = layout.glyphIndexForCharacter(at: at)
        let line = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let location = layout.location(forGlyphAt: glyph)
        let origin = textContainerOrigin
        let baseline = origin.y + line.minY + location.y
        return NSPoint(
            x: origin.x + line.minX + location.x + MarkdownStyler.markupWidth / 2,
            y: baseline - MarkdownStyler.bodyFont.capHeight / 2
        )
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self, event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command
        else { return super.performKeyEquivalent(with: event) }
        switch event.charactersIgnoringModifiers {
        case "b": wrapSelection("**")
        case "i": wrapSelection("*")
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }

    /// Return inside a list item starts the next item; on an empty item it ends the list.
    func continueList() -> Bool {
        let ns = string as NSString
        let caret = selectedRange().location
        let lineRange = ns.lineRange(for: NSRange(location: caret, length: 0))
        let line = ns.substring(with: NSRange(location: lineRange.location, length: caret - lineRange.location))
        guard let match = line.firstMatch(of: #/^(\s*)([-*+] \[[ xX]\] |[-*+] |(\d+)\. )/#) else { return false }
        let marker = String(match.output.0)
        if line == marker {
            // Empty item: drop the marker instead of adding another.
            replace(NSRange(location: lineRange.location, length: (marker as NSString).length), with: "")
            return true
        }
        var next = String(match.output.1) + String(match.output.2)
        if let number = match.output.3.flatMap({ Int($0) }) {
            next = String(match.output.1) + "\(number + 1). "
        } else if next.contains("[") {
            next = String(match.output.1) + String(match.output.2.prefix(1)) + " [ ] "
        }
        insertText("\n" + next, replacementRange: selectedRange())
        return true
    }

    private func wrapSelection(_ marker: String) {
        let range = selectedRange()
        let selected = (string as NSString).substring(with: range)
        replace(range, with: marker + selected + marker)
        setSelectedRange(NSRange(location: range.location + (marker as NSString).length, length: range.length))
    }

    /// An undoable edit that goes through the delegate like typing.
    func replace(_ range: NSRange, with text: String) {
        guard shouldChangeText(in: range, replacementString: text) else { return }
        textStorage?.replaceCharacters(in: range, with: text)
        didChangeText()
    }
}

/// A checklist box (`[ ]` / `[x]` at `at`) or `-` bullet the editor draws itself.
enum ListMark {
    case box(at: Int, checked: Bool)
    case dot(at: Int)

    static func from(_ spans: [MarkdownSpan]) -> [ListMark] {
        let boxStarts = Set(spans.compactMap { span -> Int? in
            if case .checkbox = span.style { Int(span.start) } else { nil }
        })
        return spans.compactMap { span in
            switch span.style {
            case let .checkbox(checked): .box(at: Int(span.start), checked: checked)
            // Bullets of checklist items are hidden, not drawn; numbers stay text.
            case .bullet where span.end - span.start == 1 && !boxStarts.contains(Int(span.start) + 2):
                .dot(at: Int(span.start))
            default: nil
            }
        }
    }
}

/// Turns core markdown spans into text attributes.
@MainActor
enum MarkdownStyler {
    /// Box markup is monospaced so ticking it doesn't move the text after it.
    static let markupFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
    static var markupWidth: CGFloat { ("[ ]" as NSString).size(withAttributes: [.font: markupFont]).width }
    static let bodySize: CGFloat = 15
    static var bodyFont: NSFont { .systemFont(ofSize: bodySize) }
    static var base: [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 3
        paragraph.paragraphSpacing = 4
        return [.font: bodyFont, .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph]
    }

    static func apply(to textView: NSTextView) {
        guard let storage = textView.textStorage else { return }
        let full = NSRange(location: 0, length: storage.length)
        let spans = markdownSpans(text: storage.string).map {
            (range: NSRange(location: Int($0.start), length: Int($0.end - $0.start)), style: $0.style)
        }
        let marks = ListMark.from(markdownSpans(text: storage.string))
        storage.beginEditing()
        storage.setAttributes(base, range: full)
        if let title = titleRange(storage.string as NSString, spans: spans) {
            storage.addAttribute(.font, value: NSFont.systemFont(ofSize: 22, weight: .semibold), range: title)
        }
        // Fonts first (headings, code), then traits on top of them, then colors and decorations.
        for (range, style) in spans {
            switch style {
            case let .heading(level):
                let size: CGFloat = [26, 21, 17][Int(level) - 1]
                storage.addAttribute(.font, value: NSFont.systemFont(ofSize: size, weight: level == 3 ? .semibold : .bold), range: range)
            case .code:
                storage.addAttribute(.font, value: NSFont.monospacedSystemFont(ofSize: bodySize - 1, weight: .regular), range: range)
                storage.addAttribute(.backgroundColor, value: NSColor.quaternaryLabelColor.withAlphaComponent(0.12), range: range)
            default: break
            }
        }
        for (range, style) in spans {
            let trait: NSFontTraitMask? = switch style {
            case .bold: .boldFontMask
            case .italic, .quote: .italicFontMask
            default: nil
            }
            guard let trait else { continue }
            storage.enumerateAttribute(.font, in: range) { value, run, _ in
                guard let font = value as? NSFont else { return }
                storage.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: trait), range: run)
            }
        }
        for (range, style) in spans {
            switch style {
            case .syntax:
                storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: range)
            case .quote:
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            case .bullet:
                storage.addAttribute(.foregroundColor, value: NSColor.controlAccentColor, range: range)
            case .checkbox:
                storage.addAttribute(.cursor, value: NSCursor.pointingHand, range: range)
            case .done, .strike:
                storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            default: break
            }
        }
        // Hide the markup the editor draws over. A box's bullet and space collapse so boxes line up
        // with bullets; a bullet is kerned as wide as box markup so their text lines up too.
        for mark in marks {
            switch mark {
            case let .box(at, _):
                let bullet = NSRange(location: max(at - 2, 0), length: min(2, at))
                storage.addAttributes([.font: NSFont.systemFont(ofSize: 0.1), .foregroundColor: NSColor.clear], range: bullet)
                storage.addAttributes([.font: markupFont, .foregroundColor: NSColor.clear], range: NSRange(location: at, length: 3))
            case let .dot(at):
                let dash = ("-" as NSString).size(withAttributes: [.font: markupFont]).width
                storage.addAttributes(
                    [.font: markupFont, .foregroundColor: NSColor.clear, .kern: markupWidth - dash],
                    range: NSRange(location: at, length: 1)
                )
            }
        }
        // Image lines: markup hidden, the line as tall as the picture at the editor's width.
        let images = markdownImages(text: storage.string)
        let width = max((textView.textContainer?.size.width ?? 400) - 2 * (textView.textContainer?.lineFragmentPadding ?? 0), 100)
        for image in images {
            let range = NSRange(location: Int(image.start), length: Int(image.end - image.start))
            var height = imagePlaceholderHeight
            if let picture = NoteImageCache.shared.image(image.id), picture.size.width > 0 {
                height = min(width * picture.size.height / picture.size.width, imageMaxHeight)
            }
            let paragraph = NSMutableParagraphStyle()
            paragraph.minimumLineHeight = height + imageGap * 2
            paragraph.maximumLineHeight = height + imageGap * 2
            paragraph.lineSpacing = 3
            paragraph.paragraphSpacing = 4
            storage.addAttributes([.font: NSFont.systemFont(ofSize: 0.1), .foregroundColor: NSColor.clear, .paragraphStyle: paragraph], range: range)
        }
        storage.endEditing()
        textView.typingAttributes = base
        (textView as? MarkdownTextView)?.marks = marks
        (textView as? MarkdownTextView)?.imageLines = images.map {
            (NSRange(location: Int($0.start), length: Int($0.end - $0.start)), $0.id)
        }
    }

    static let imageGap: CGFloat = 6
    static let imagePlaceholderHeight: CGFloat = 160
    static let imageMaxHeight: CGFloat = 520

    /// The first non-blank line, which titles the note, unless it is a heading, quote or list item.
    private static func titleRange(_ text: NSString, spans: [(range: NSRange, style: MarkdownStyle)]) -> NSRange? {
        var start = 0
        while start < text.length {
            let line = text.lineRange(for: NSRange(location: start, length: 0))
            let content = text.substring(with: line).trimmingCharacters(in: .newlines)
            if !content.trimmingCharacters(in: .whitespaces).isEmpty {
                let marked = spans.contains { span in
                    guard span.range.location == start else { return false }
                    return switch span.style {
                    case .heading, .bullet, .syntax: true
                    default: false
                    }
                }
                return marked ? nil : NSRange(location: start, length: (content as NSString).length)
            }
            start = NSMaxRange(line)
        }
        return nil
    }
}

/// A menu item that runs a closure, for menus built on the fly.
final class ClosureMenuItem: NSMenuItem {
    private let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used") }

    @objc private func fire() { run() }
}
