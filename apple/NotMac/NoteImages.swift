import AppKit
import ImageIO
import NotCore
import UniformTypeIdentifiers

// Images in notes: pasted, dropped or picked, downscaled and re-encoded here, then kept by the core
// under a content id (ADR 0010). Editors draw them over their hidden `![](not-image:…)` line.

enum NoteImageEncoder {
    /// Longest side kept: sharp on a laptop or phone screen, a few hundred KB as JPEG.
    static let maxSide = 2048
    static let jpegQuality = 0.85

    /// Any image ImageIO reads (JPEG, PNG, HEIC, TIFF from the pasteboard…), EXIF rotation applied,
    /// as JPEG — or PNG when it has transparency.
    static func encode(_ data: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxSide,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let alpha: Bool = switch image.alphaInfo {
        case .none, .noneSkipFirst, .noneSkipLast: false
        default: true
        }
        let out = NSMutableData()
        let type = (alpha ? UTType.png : UTType.jpeg).identifier as CFString
        guard let destination = CGImageDestinationCreateWithData(out, type, 1, nil) else { return nil }
        let properties: [CFString: Any] = alpha ? [:] : [kCGImageDestinationLossyCompressionQuality: jpegQuality]
        CGImageDestinationAddImage(destination, image, properties as CFDictionary)
        return CGImageDestinationFinalize(destination) ? out as Data : nil
    }
}

/// Decoded images by id, shared by every editor.
@MainActor
final class NoteImageCache {
    static let shared = NoteImageCache()
    private let cache = NSCache<NSString, NSImage>()
    /// Reads an image's bytes from the store; set by the app model.
    var load: ((String) -> Data?)?

    /// `nil` while the image hasn't arrived from the server.
    func image(_ id: String) -> NSImage? {
        if let image = cache.object(forKey: id as NSString) { return image }
        guard let data = load?(id), let image = NSImage(data: data) else { return nil }
        cache.setObject(image, forKey: id as NSString)
        return image
    }
}
