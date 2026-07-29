#!/usr/bin/env bash
#
# Check that a built app is signed, hardened, notarized and stapled — the four
# things that decide whether someone else's Mac will open it without a fight.
#
# Run:  ./scripts/verify-signing.sh [path/to/Amble.app]

set -uo pipefail

APP="${1:-src-tauri/target/release/bundle/macos/Amble.app}"
ok()   { printf "  \033[32m✓\033[0m %s\n" "$*"; }
bad()  { printf "  \033[31m✗\033[0m %s\n" "$*"; }
warn() { printf "  \033[33m!\033[0m %s\n" "$*"; }

printf "\033[1mVerifying %s\033[0m\n\n" "$APP"
[[ -d "$APP" ]] || { bad "Not found. Build it first: npm run tauri build"; exit 1; }

FAILED=0

# 1. Signed with a real identity, not ad-hoc.
SIG="$(codesign -dvv "$APP" 2>&1)"
if grep -q "Signature=adhoc" <<<"$SIG"; then
  bad "Ad-hoc signed — Gatekeeper will block this, and TCC will forget permissions on every rebuild"
  FAILED=1
elif AUTH="$(grep -m1 'Authority=' <<<"$SIG" | cut -d= -f2-)"; [[ -n "$AUTH" ]]; then
  ok "Signed by: $AUTH"
else
  bad "Unsigned"
  FAILED=1
fi

# 2. Hardened runtime — notarization is refused without it.
if codesign -d --verbose=2 "$APP" 2>&1 | grep -q "flags=.*runtime"; then
  ok "Hardened runtime enabled"
else
  warn "Hardened runtime NOT enabled — Apple will refuse to notarize"
  FAILED=1
fi

# 3. Entitlements actually applied.
ENTS="$(codesign -d --entitlements - --xml "$APP" 2>/dev/null | tr -d '\0')"
if grep -q "com.apple.security.device.audio-input" <<<"$ENTS"; then
  ok "Microphone entitlement present"
else
  warn "Microphone entitlement missing from the signature"
fi

# 4. Signature is internally valid.
if codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | grep -q "satisfies its Designated Requirement"; then
  ok "Signature valid"
else
  bad "Signature invalid"
  FAILED=1
fi

# 5. The one that actually decides the download experience.
ASSESS="$(spctl -a -vvv -t install "$APP" 2>&1)"
if grep -q "accepted" <<<"$ASSESS"; then
  ok "Gatekeeper: accepted — opens with no warning"
  grep -q "Notarized Developer ID" <<<"$ASSESS" && ok "Notarized" || warn "Signed but not notarized"
else
  bad "Gatekeeper: rejected — users will need the right-click → Open dance"
  printf "      %s\n" "$(head -2 <<<"$ASSESS")"
  FAILED=1
fi

# 6. Stapled, so it validates offline.
if xcrun stapler validate "$APP" >/dev/null 2>&1; then
  ok "Notarization ticket stapled (validates offline)"
else
  warn "No stapled ticket — first launch will need a network round-trip to Apple"
fi

echo
if [[ $FAILED -eq 0 ]]; then
  printf "  \033[32mReady to ship.\033[0m\n"
else
  printf "  \033[33mNot ready — see above. ./scripts/setup-signing.sh walks through the fixes.\033[0m\n"
  exit 1
fi
