# Release setup — one-time

This wires up signed + notarized auto-updates. After this is done, the
release flow is just:

```bash
git tag v0.5.1 && git push origin v0.5.1
```

The GitHub Action builds, signs, notarizes, uploads the DMG to a Release,
and writes the `latest.json` the in-app updater fetches. Existing installs
prompt the user the next time Dispatch launches.

## Pieces (all already in the repo)

- `tauri-plugin-updater` registered in `src-tauri/src/lib.rs`
- Endpoint + pubkey configured in `src-tauri/tauri.conf.json` → `plugins.updater`
- `process:allow-restart` capability so the app can relaunch after install
- `src/composables/useAutoUpdate.ts` checks on startup, surfaces a toast
- `.github/workflows/release.yml` builds + signs on `v*` tag push

## Updater signing keypair (DONE — keep these safe!)

Generated at `~/.tauri/dispatch.key` (private) + `.pub` (committed inline
in `tauri.conf.json`). **The private key was generated without a password.**

**Back up the private key now:**
1. 1Password → New item → Secure Note → "Dispatch updater signing key"
2. Paste contents of `~/.tauri/dispatch.key` into the note body
3. Save

**If you lose this key, you cannot ship updates to existing installs.** New
installs would still get the new pubkey, but old ones would reject every
future update because the signature wouldn't verify.

## GitHub Secrets to add

Repo → Settings → Secrets and variables → Actions → New repository secret.

| Name | Value | Where it comes from |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | full contents of `~/.tauri/dispatch.key` | `cat ~/.tauri/dispatch.key \| pbcopy` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | _blank_ | (generated without password) |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: EJ Fox (TEAMID)` | Keychain → your Developer ID Application cert → "Common Name" + paren team ID |
| `APPLE_CERTIFICATE` | base64 of the .p12 export | see below |
| `APPLE_CERTIFICATE_PASSWORD` | password you set when exporting .p12 | you choose |
| `APPLE_ID` | your dev-account email | |
| `APPLE_PASSWORD` | app-specific password (NOT your Apple ID password) | https://appleid.apple.com → Sign-In and Security → App-Specific Passwords |
| `APPLE_TEAM_ID` | 10-char team ID | https://developer.apple.com/account → Membership Details |

### Exporting `APPLE_CERTIFICATE`

In Keychain Access:
1. Find "Developer ID Application: EJ Fox (TEAMID)"
2. Right-click → Export → Save as `.p12` (set a password — that's `APPLE_CERTIFICATE_PASSWORD`)
3. `base64 -i ~/Downloads/cert.p12 | pbcopy`
4. Paste into `APPLE_CERTIFICATE` secret

## Releasing

```bash
# bump version in package.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json
# update CHANGELOG.md with the new entry
git add -A && git commit -m "chore: 0.5.1 release"
git tag v0.5.1
git push origin main v0.5.1
```

The Action runs ~10 minutes. Watch it at github.com/ejfox/website-dispatch/actions.
When it's green, the in-app updater on every running Dispatch will see the
new release within a few minutes.

## Local testing without releasing

```bash
# Build a signed DMG locally (needs only the Apple env vars):
export APPLE_SIGNING_IDENTITY="Developer ID Application: EJ Fox (TEAMID)"
export APPLE_ID=ejfox@ejfox.com
export APPLE_PASSWORD="<app-specific-password>"
export APPLE_TEAM_ID="<TEAMID>"
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/dispatch.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

npm run tauri build
```

Stick those exports in `~/.zshrc` (or use direnv) so any future local
`tauri build` produces a properly signed + notarized DMG without manual
`xattr -cr` dance.
