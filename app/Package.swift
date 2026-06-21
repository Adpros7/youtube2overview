// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "yt2overview",
    platforms: [
        .macOS(.v26)
    ],
    targets: [
        .executableTarget(
            name: "yt2overview",
            path: "Sources/yt2overview"
        )
    ]
)
