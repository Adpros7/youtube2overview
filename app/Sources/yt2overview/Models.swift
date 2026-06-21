import Foundation

// MARK: - Settings (mirrors backend config.rs; JSON is snake_case)

enum CommentSort: String, Codable, CaseIterable, Identifiable {
    case top, new
    var id: String { rawValue }
    var label: String { self == .top ? "Top" : "Newest" }
}

enum FrameStrategy: String, Codable, CaseIterable, Identifiable {
    case even, chapters
    case sceneChange = "scene_change"
    var id: String { rawValue }
    var label: String {
        switch self {
        case .even: return "Evenly spaced"
        case .chapters: return "Per chapter"
        case .sceneChange: return "Scene changes"
        }
    }
}

enum OverviewLength: String, Codable, CaseIterable, Identifiable {
    case brief, standard, detailed
    var id: String { rawValue }
    var label: String { rawValue.capitalized }
}

struct Sections: Codable, Equatable {
    var aiPreamble = true
    var metadata = true
    var chapters = true
    var aiOverview = true
    var visualOverview = true
    var comments = true
    var transcript = true
}

struct Settings: Codable, Equatable {
    var model = "mlx-community/gemma-4-12b-it-4bit"
    var whisperModel = "mlx-community/whisper-large-v3-turbo"
    var mlxPort: Int = 0
    var temperature: Double = 0.4
    var maxTokens: Int = 1536

    var includeComments = true
    var maxComments: Int = 20
    var commentSort: CommentSort = .top

    var includeVisual = true
    var maxFrames: Int = 8
    var frameStrategy: FrameStrategy = .even

    var overviewLength: OverviewLength = .standard
    var overviewStyle = "neutral, informative"
    var language = ""

    var includeTranscript = true
    var transcriptTimestamps = true

    var sections = Sections()
}

// MARK: - Results (mirrors backend model.rs)

struct VideoMeta: Codable, Equatable {
    var id = ""
    var title = ""
    var uploader = ""
    var channel = ""
    var duration: Double = 0
    var viewCount: Int = 0
    var likeCount: Int = 0
    var uploadDate = ""
    var webpageUrl = ""
    var thumbnail = ""
    var description = ""
}

struct Chapter: Codable, Equatable, Identifiable {
    var title = ""
    var start: Double = 0
    var end: Double = 0
    var id: String { "\(start)-\(title)" }
}

struct Comment: Codable, Equatable, Identifiable {
    var author = ""
    var text = ""
    var likes: Int = 0
    var isFavorited = false
    var id: String { author + String(text.prefix(16)) }
}

struct OutputSection: Codable, Equatable, Identifiable {
    var id: String
    var title: String
    var markdown: String
}

struct Outputs: Codable, Equatable {
    var humanMarkdown = ""
    var aiPayload = ""
    var sections: [OutputSection] = []
}

struct JobData: Codable, Equatable {
    var meta = VideoMeta()
    var chapters: [Chapter] = []
    var comments: [Comment] = []
    var aiOverview = ""
    var visualOverview = ""
    var frameCount = 0
    var modelUsed = ""
    var transcriptLang = ""
}

struct JobResult: Codable, Equatable {
    var data = JobData()
    var outputs = Outputs()
    var settings = Settings()
}

struct CachedModel: Codable, Identifiable {
    var repo: String
    var alias: String?
    var size: String
    var multimodal: Bool
    var id: String { repo }
}

struct ProgressEvent: Codable {
    var stage: String
    var message: String
    var progress: Double
    var kind: String
}

// MARK: - JSON helpers

enum JSON {
    static var encoder: JSONEncoder {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        return e
    }
    static var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }
}
