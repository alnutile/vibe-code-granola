#!/usr/bin/env bash
#
# One-time signing setup for local release builds.
#
# Signing turns two annoyances off at once:
#   * Gatekeeper stops blocking the app for everyone who downloads it.
#   * macOS stops forgetting Screen Recording permission on every rebuild —
#     TCC keys the grant to the signing certificate rather than the exact
#     binary, so a rebuild keeps it.
#
# Run:  ./scripts/setup-signing.sh
#
# Nothing here is destructive, and it can be re-run safely.

set -euo pipefail

bold() { printf "\033[1m%s\033[0m\n" "$*"; }
ok()   { printf "  \033[32m✓\033[0m %s\n" "$*"; }
warn() { printf "  \033[33m!\033[0m %s\n" "$*"; }
step() { printf "\n\033[1m%s\033[0m\n" "$*"; }

bold "Amble — code signing setup"

# ── 1. Developer ID Application certificate ──────────────────────────────────
step "1. Developer ID Application certificate"

IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
  | grep "Developer ID Application" \
  | head -1 \
  | sed -E 's/.*"(.*)".*/\1/')"

if [[ -z "$IDENTITY" ]]; then
  warn "Not found — this is the one part Apple makes you do by hand."
  cat <<'EOS'

  The quickest route (Xcode generates the key, CSR and installs the cert):

    Xcode → Settings → Accounts
      → select your team
      → Manage Certificates…
      → the + button, bottom-left
      → Developer ID Application

  If your Apple ID isn't listed under Accounts yet, add it there first.

  Then re-run this script.

EOS
  exit 1
fi

ok "$IDENTITY"

# The team id is the parenthesised suffix of the certificate name.
TEAM_ID="$(sed -E 's/.*\(([A-Z0-9]+)\)$/\1/' <<<"$IDENTITY")"
ok "Team ID: $TEAM_ID"

# ── 2. Notarization credentials ──────────────────────────────────────────────
step "2. Notarization credentials"

PROFILE="amble-notary"

if xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  ok "Keychain profile '$PROFILE' already works"
else
  warn "No stored credentials yet."
  cat <<EOS

  Notarizing needs an app-specific password (NOT your Apple ID password):

    appleid.apple.com → Sign-In and Security → App-Specific Passwords → +

  Then paste it when prompted below. It is stored in your Keychain, not on disk.

EOS
  read -rp "  Apple ID email: " APPLE_ID_EMAIL
  # notarytool prompts for the password itself and never echoes it.
  xcrun notarytool store-credentials "$PROFILE" \
    --apple-id "$APPLE_ID_EMAIL" \
    --team-id "$TEAM_ID"
  ok "Stored as Keychain profile '$PROFILE'"
fi

# ── 3. Local build environment ───────────────────────────────────────────────
step "3. Building signed + notarized locally"

ENV_FILE=".env.signing"
cat > "$ENV_FILE" <<EOS
# Source this before 'npm run tauri build' to produce a signed, notarized app.
#   source .env.signing && npm run tauri build
#
# Gitignored — APPLE_PASSWORD is a real credential.
export APPLE_SIGNING_IDENTITY="$IDENTITY"
export APPLE_TEAM_ID="$TEAM_ID"

# Fill these in to notarize. Same app-specific password as above.
export APPLE_ID="${APPLE_ID_EMAIL:-your-apple-id@example.com}"
export APPLE_PASSWORD="paste-your-app-specific-password"
EOS
chmod 600 "$ENV_FILE"

ok "Wrote $ENV_FILE (gitignored)"
cat <<EOS

  Add your app-specific password to $ENV_FILE, then:

    source $ENV_FILE && npm run tauri build

  Tauri signs the app and submits it to Apple automatically. Notarization
  usually takes 1–5 minutes; the bundler waits for it and staples the ticket.

  To verify afterwards:

    ./scripts/verify-signing.sh

  For CI, upload the same material to GitHub with:

    ./scripts/upload-signing-secrets.sh

EOS
