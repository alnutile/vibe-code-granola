#!/usr/bin/env bash
#
# Push your signing material to GitHub Actions secrets, so tagged releases come
# out signed and notarized instead of quarantined.
#
# Run:  ./scripts/upload-signing-secrets.sh
#
# The private key leaves your Keychain only as an encrypted .p12, written to a
# 0600 temp file that is deleted on exit — including if this script fails.

set -euo pipefail

ok()   { printf "  \033[32m✓\033[0m %s\n" "$*"; }
warn() { printf "  \033[33m!\033[0m %s\n" "$*"; }
die()  { printf "  \033[31m✗\033[0m %s\n" "$*" >&2; exit 1; }

printf "\033[1mUploading signing secrets to GitHub\033[0m\n\n"

command -v gh >/dev/null || die "The GitHub CLI (gh) is required."
gh auth status >/dev/null 2>&1 || die "Run 'gh auth login' first."

IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
  | grep "Developer ID Application" | head -1 | sed -E 's/.*"(.*)".*/\1/')"
[[ -n "$IDENTITY" ]] || die "No Developer ID Application certificate. Run ./scripts/setup-signing.sh first."

TEAM_ID="$(sed -E 's/.*\(([A-Z0-9]+)\)$/\1/' <<<"$IDENTITY")"
ok "Identity: $IDENTITY"

TMP="$(mktemp -d)"
# Deleted even on failure — this directory holds the private key.
trap 'rm -rf "$TMP"' EXIT
P12="$TMP/certificate.p12"

# A transport password for the .p12 itself. Random: it is only ever used to move
# the key to GitHub, and is stored alongside it as a secret.
P12_PASSWORD="$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32)"

printf "\n  Exporting the certificate from your Keychain.\n"
printf "  macOS will ask for your login password to release the private key.\n\n"

security export -t identities -f pkcs12 -P "$P12_PASSWORD" -o "$P12" \
  2>/dev/null || die "Export failed — approve the Keychain prompt and re-run."

[[ -s "$P12" ]] || die "Exported .p12 is empty."
chmod 600 "$P12"
ok "Exported ($(wc -c <"$P12" | tr -d ' ') bytes)"

read -rp "  Apple ID email (for notarization): " APPLE_ID_EMAIL
read -rsp "  App-specific password: " APPLE_APP_PASSWORD; echo

REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
printf "\n  Setting secrets on %s…\n" "$REPO"

set_secret() {
  printf '%s' "$2" | gh secret set "$1" --repo "$REPO" >/dev/null
  ok "$1"
}

set_secret APPLE_CERTIFICATE          "$(base64 -i "$P12")"
set_secret APPLE_CERTIFICATE_PASSWORD "$P12_PASSWORD"
set_secret APPLE_SIGNING_IDENTITY     "$IDENTITY"
set_secret APPLE_ID                   "$APPLE_ID_EMAIL"
set_secret APPLE_PASSWORD             "$APPLE_APP_PASSWORD"
set_secret APPLE_TEAM_ID              "$TEAM_ID"

cat <<EOS

  Done. The next tagged release will be signed and notarized:

    git tag v0.1.1 && git push origin v0.1.1

  The workflow skips signing automatically if these secrets are ever removed,
  so it still builds (unsigned) for forks and contributors.

EOS
