# Desktop Packaging, Signing, And Updates

Sprint 6 ships local Tauri packaging scaffolding for the desktop GUI. Packages
are pre-release until signing, notarization, and updater infrastructure are
configured with production credentials.

## Local Build

Build the WebUI and macOS bundle from this workspace:

```bash
npm run build --prefix crates/webui
cd crates/gui
../webui/node_modules/.bin/tauri build
```

On this machine, the Tauri build produced:

```text
target/release/bundle/dmg/NightBridge_26.5.0_aarch64.dmg
```

The Tauri CLI runs `beforeBuildCommand` from the `crates/` directory, so the
configured command is:

```json
"beforeBuildCommand": "npm run build --prefix webui"
```

## macOS Signing And Notarization

Production macOS distribution still needs Apple Developer signing and
notarization. Required values must be provided by CI or the local shell and must
not be committed:

- Apple Developer Program membership
- Developer ID Application certificate
- `APPLE_ID`
- `APPLE_PASSWORD` or app-specific password
- `APPLE_TEAM_ID`
- signing identity or certificate import material
- keychain password and temporary keychain path, when CI imports certificates

Current cost and requirement snapshot:

- Apple Developer Program membership is USD 99 per membership year.
- For Mac software distributed outside the Mac App Store, Apple documents that
  developers need Apple Developer Program membership, a Developer ID
  certificate, and notarization.
- References:
  `https://developer.apple.com/programs/` and
  `https://developer.apple.com/support/developer-id/`

Until those values are configured, local `.app` and `.dmg` builds are unsigned
developer artifacts.

Decision for 26.5: macOS desktop artifacts are scoped as unsigned pre-release
builds. Production notarized distribution is deferred until Apple Developer
credentials and a signing workflow are available.

## Windows Authenticode

Windows packages require either an Authenticode code-signing certificate from a
trusted CA or a managed signing service such as Azure Artifact Signing. Required
values must be injected as CI secrets or local environment variables:

- certificate path or base64-encoded certificate secret
- certificate password secret
- timestamp server URL
- signing identity metadata

Do not commit `.pfx`, `.p12`, private keys, passwords, or timestamping tokens.

Current cost and requirement snapshot:

- Microsoft lists Azure Artifact Signing as a managed code-signing option with
  a Basic plan at USD 9.99 per month for up to 5,000 signatures, plus overage.
- Microsoft Learn lists Azure Artifact Signing as Microsoft's recommended
  signing service for developers distributing apps outside the Microsoft Store.
- Traditional Authenticode certificates are also valid, but pricing and
  issuance rules depend on the certificate authority.
- References:
  `https://azure.microsoft.com/en-us/products/artifact-signing` and
  `https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options`

Decision for 26.5: Windows desktop signing is deferred. Windows packages, if
produced before signing is configured, must be labeled unsigned pre-release
artifacts.

## Linux AppImage

Linux AppImage packaging should be run on Linux with the Tauri system
dependencies installed. At minimum, CI or the packaging host needs WebKitGTK,
GTK, AppImage tooling, and the distro packages required by Tauri 2 for Linux
bundles.

This macOS machine did not validate Linux AppImage output.

## Updater Boundary

No production updater endpoint is configured in Sprint 6. This is deliberate:
there is no signed public release channel yet, and a placeholder endpoint could
make pre-release builds look update-capable when they are not.

When updater support is added, store the updater private key only as a CI secret
or local secret. Public update metadata should be generated as part of a signed
release job, not by regular development builds.

Decision for 26.5: updater publishing is deferred until signing is configured.

## Local Gaps Recorded

- macOS `.dmg` packaging passes locally for `aarch64-apple-darwin`.
- macOS notarization was not attempted because Apple Developer credentials are
  not configured in this workspace.
- Windows MSI packaging was not attempted on this macOS host.
- Windows Authenticode signing was not attempted because no signing certificate
  is configured.
- Linux AppImage packaging was not attempted on this macOS host.
- Updater signing and update metadata are not configured for Sprint 6.
