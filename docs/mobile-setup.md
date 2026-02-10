# MarlOS Mobile Setup Guide

This guide covers setting up MarlOS for iOS and Android development using Tauri 2.0.

## Prerequisites

### All Platforms
- Node.js 18+
- Rust (latest stable)
- Tauri CLI: `npm install -g @tauri-apps/cli`

### Android Development

#### Windows/Mac/Linux

1. **Install Android Studio**
   - Download from https://developer.android.com/studio
   - During installation, select:
     - Android SDK
     - Android SDK Platform-Tools
     - Android Virtual Device (emulator)

2. **Configure Environment Variables**
   ```bash
   # Windows (add to Environment Variables)
   ANDROID_HOME=C:\Users\<user>\AppData\Local\Android\Sdk
   JAVA_HOME=C:\Program Files\Android\Android Studio\jbr

   # Mac/Linux (add to ~/.bashrc or ~/.zshrc)
   export ANDROID_HOME=$HOME/Android/Sdk
   export JAVA_HOME=/Applications/Android\ Studio.app/Contents/jbr/Contents/Home
   export PATH=$PATH:$ANDROID_HOME/platform-tools
   export PATH=$PATH:$ANDROID_HOME/tools
   ```

3. **Install Android NDK**
   - Open Android Studio → Settings → SDK Manager
   - SDK Tools tab → Check "NDK (Side by side)"
   - Install version 26.x or later

4. **Add Rust Android Targets**
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
   ```

5. **Initialize Tauri Android**
   ```bash
   cd marlos-rust
   npx tauri android init
   ```

### iOS Development (Mac Only)

1. **Install Xcode**
   - Download from Mac App Store
   - Open Xcode and accept license
   - Install command line tools: `xcode-select --install`

2. **Add Rust iOS Targets**
   ```bash
   rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
   ```

3. **Install CocoaPods** (if needed)
   ```bash
   sudo gem install cocoapods
   ```

4. **Initialize Tauri iOS**
   ```bash
   cd marlos-rust
   npx tauri ios init
   ```

5. **Configure Development Team**
   - Edit `src-tauri/tauri.conf.json`
   - Set `bundle.iOS.developmentTeam` to your Apple Developer Team ID

## Building

### Android

```bash
# Development build
npx tauri android dev

# Run on connected device
npx tauri android dev --device

# Release build (APK)
npx tauri android build

# Release build (AAB for Play Store)
npx tauri android build --aab
```

### iOS

```bash
# Development build (simulator)
npx tauri ios dev

# Run on connected device
npx tauri ios dev --device

# Release build
npx tauri ios build
```

## Mobile-Specific Considerations

### Storage Paths

On mobile, the app data directory is different:
- **Android**: `/data/data/com.marlos.app/files/`
- **iOS**: `~/Library/Application Support/com.marlos.app/`

The app automatically detects the platform and uses the correct path.

### Embedding Provider

On mobile, there's no LM Studio running locally. Options:

1. **Cloud Embeddings** - Use OpenAI API or similar
2. **On-Device Models** - Use ONNX Runtime with a small model
3. **Sync Only** - Mobile syncs with desktop, doesn't generate embeddings

For now, the mobile app will work in "sync mode" - viewing and searching existing data, with full AI features requiring a connected server.

### UI Adaptations

The SolidJS frontend automatically adapts to mobile screens:
- Sidebar collapses to hamburger menu
- Touch-friendly buttons and gestures
- Responsive layouts

### Network

Mobile apps may need to connect to:
- Desktop MarlOS instance for sync (future)
- Cloud API for embeddings (optional)

## Project Structure After Init

```
src-tauri/
├── gen/
│   ├── android/           # Android project files
│   │   ├── app/
│   │   │   └── src/main/
│   │   │       ├── AndroidManifest.xml
│   │   │       └── java/com/marlos/app/
│   │   ├── build.gradle.kts
│   │   └── settings.gradle.kts
│   └── apple/             # iOS project files (Mac only)
│       ├── MarlOS.xcodeproj/
│       └── MarlOS/
├── Cargo.toml
└── tauri.conf.json
```

## Icons

Create app icons at these sizes:

### Android (in `src-tauri/gen/android/app/src/main/res/`)
- `mipmap-mdpi/`: 48x48
- `mipmap-hdpi/`: 72x72
- `mipmap-xhdpi/`: 96x96
- `mipmap-xxhdpi/`: 144x144
- `mipmap-xxxhdpi/`: 192x192

### iOS (in `src-tauri/gen/apple/MarlOS/Assets.xcassets/AppIcon.appiconset/`)
- 20, 29, 40, 58, 60, 76, 80, 87, 120, 152, 167, 180, 1024 (px)

## Signing

### Android

For release builds, create a keystore:

```bash
keytool -genkey -v -keystore marlos-release.keystore -alias marlos -keyalg RSA -keysize 2048 -validity 10000
```

Add to `src-tauri/gen/android/app/build.gradle.kts`:

```kotlin
android {
    signingConfigs {
        create("release") {
            storeFile = file("marlos-release.keystore")
            storePassword = System.getenv("KEYSTORE_PASSWORD")
            keyAlias = "marlos"
            keyPassword = System.getenv("KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("release")
        }
    }
}
```

### iOS

1. Enroll in Apple Developer Program ($99/year)
2. Create App ID in Apple Developer Portal
3. Create provisioning profiles
4. Add team ID to `tauri.conf.json`

## Troubleshooting

### Android: "SDK location not found"
- Check `ANDROID_HOME` environment variable
- Create `local.properties` in `src-tauri/gen/android/` with `sdk.dir=/path/to/sdk`

### iOS: "No provisioning profile"
- Open project in Xcode
- Select team in Signing & Capabilities
- Let Xcode manage signing

### Build Errors
```bash
# Clean and rebuild
npx tauri android dev --no-cache

# Check Rust targets
rustup target list --installed
```

## Future: Desktop-Mobile Sync

Planned sync architecture:

```
Desktop MarlOS
     │
     ├── Local SQLite DB
     ├── Local Embeddings
     │
     ▼
Sync Server (optional)
     │
     ▼
Mobile MarlOS
     │
     ├── Local SQLite (synced subset)
     └── Search via server or local
```

Mobile will be able to:
- Browse synced sessions and decisions
- Search (via server or cached embeddings)
- View research and papers
- Quick capture notes (sync back to desktop)
