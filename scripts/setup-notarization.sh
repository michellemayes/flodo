#!/usr/bin/env bash
#
# One-time setup so every release is Developer ID signed and notarized.
#
# You do one thing by hand: create an App Store Connect API key (Users and
# Access > Integrations > App Store Connect API > Team Keys, role "Admin"),
# download its AuthKey_XXXXXXXXXX.p8, and note the Key ID and Issuer ID shown
# on that page. Then:
#
#   ./scripts/setup-notarization.sh --key-id ABC123DEFG \
#       --issuer 69a6de7e-0000-0000-0000-000000000000 \
#       --p8 ~/Downloads/AuthKey_ABC123DEFG.p8
#
# The script uses that key to create a "Developer ID Application" certificate
# through the App Store Connect API (the private key never leaves this
# machine), bundles it into a .p12, and stores all five GitHub secrets the
# release workflow reads. It needs openssl, python3 and an authenticated `gh`.
#
# Already have a Developer ID certificate? Export it from Keychain Access as a
# .p12 and pass `--p12 path/to/cert.p12` to skip creating a new one (Apple
# allows only a handful per account). It will ask for that file's password.
#
set -euo pipefail

KEY_ID=""
ISSUER=""
P8=""
P12=""
REPO=""
OUT="$HOME/.flodo-signing"

while [ $# -gt 0 ]; do
  case "$1" in
    --key-id) KEY_ID="${2:?}"; shift ;;
    --issuer) ISSUER="${2:?}"; shift ;;
    --p8) P8="${2:?}"; shift ;;
    --p12) P12="${2:?}"; shift ;;
    --repo) REPO="${2:?}"; shift ;;
    -h|--help) sed -n '3,22{s/^# \{0,1\}//;p;}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "setup-notarization.sh: unknown option $1" >&2; exit 1 ;;
  esac
  shift
done

die() { echo "setup-notarization.sh: $*" >&2; exit 1; }
[ -n "$KEY_ID" ] && [ -n "$ISSUER" ] && [ -n "$P8" ] || die "--key-id, --issuer and --p8 are required (see --help)"
[ -f "$P8" ] || die "no such file: $P8"
for tool in openssl python3 gh; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
gh auth status >/dev/null 2>&1 || die "run 'gh auth login' first"
REPO="${REPO:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"

umask 077
mkdir -p "$OUT"

if [ -n "$P12" ]; then
  [ -f "$P12" ] || die "no such file: $P12"
  read -rsp "Password for $P12: " P12_PASS; echo
  cp "$P12" "$OUT/developer-id.p12"
else
  echo "==> Creating a Developer ID Application certificate"
  openssl genrsa -out "$OUT/developer-id.key" 2048 2>/dev/null
  openssl req -new -key "$OUT/developer-id.key" -out "$OUT/developer-id.csr" \
    -subj "/CN=Flodo Developer ID/O=Flodo" 2>/dev/null

  # ES256 JWT for the App Store Connect API, signed by openssl so this needs
  # nothing beyond the Python standard library.
  KEY_ID="$KEY_ID" ISSUER="$ISSUER" P8="$P8" CSR="$OUT/developer-id.csr" \
    CER="$OUT/developer-id.cer" python3 - <<'PY'
import base64, json, os, subprocess, sys, time, urllib.error, urllib.request

def b64url(raw):
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()

def der_to_raw(der):
    # SEQUENCE { INTEGER r, INTEGER s } -> r || s, 32 bytes each.
    i = 2 if der[1] < 0x80 else 2 + (der[1] & 0x7F)
    out = b""
    for _ in range(2):
        assert der[i] == 0x02
        n = der[i + 1]
        out += der[i + 2 : i + 2 + n].lstrip(b"\0").rjust(32, b"\0")
        i += 2 + n
    return out

now = int(time.time())
signing_input = (
    b64url(json.dumps({"alg": "ES256", "kid": os.environ["KEY_ID"], "typ": "JWT"}).encode())
    + "."
    + b64url(json.dumps({"iss": os.environ["ISSUER"], "iat": now, "exp": now + 600,
                         "aud": "appstoreconnect-v1"}).encode())
)
der = subprocess.run(["openssl", "dgst", "-sha256", "-sign", os.environ["P8"]],
                     input=signing_input.encode(), capture_output=True, check=True).stdout
token = signing_input + "." + b64url(der_to_raw(der))

body = {"data": {"type": "certificates", "attributes": {
    "certificateType": "DEVELOPER_ID_APPLICATION",
    "csrContent": open(os.environ["CSR"]).read(),
}}}
req = urllib.request.Request(
    "https://api.appstoreconnect.apple.com/v1/certificates",
    data=json.dumps(body).encode(), method="POST",
    headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
)
try:
    with urllib.request.urlopen(req) as resp:
        cert = json.load(resp)["data"]["attributes"]["certificateContent"]
except urllib.error.HTTPError as e:
    detail = e.read().decode(errors="replace")
    sys.exit(f"App Store Connect refused to create the certificate ({e.code}):\n{detail}\n\n"
             "Developer ID certificates can only be made by the Account Holder, and an account\n"
             "holds at most five. Create one in Xcode instead (Settings > Accounts > Manage\n"
             "Certificates > + > Developer ID Application), export it from Keychain Access as a\n"
             ".p12, and re-run this script with --p12 <file>.")
open(os.environ["CER"], "wb").write(base64.b64decode(cert))
PY

  P12_PASS="$(openssl rand -hex 24)"
  # SHA1-3DES keeps the .p12 importable by `security` on the runner, whether
  # this machine's openssl is LibreSSL or OpenSSL 3.
  openssl x509 -inform DER -in "$OUT/developer-id.cer" -out "$OUT/developer-id.pem"
  openssl pkcs12 -export -inkey "$OUT/developer-id.key" -in "$OUT/developer-id.pem" \
    -name "Flodo Developer ID" -out "$OUT/developer-id.p12" -passout "pass:$P12_PASS" \
    -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
  printf '%s\n' "$P12_PASS" > "$OUT/developer-id.p12.password"
  rm -f "$OUT/developer-id.csr" "$OUT/developer-id.pem"
  echo "    $(openssl x509 -inform DER -in "$OUT/developer-id.cer" -noout -subject)"
fi

echo "==> Storing secrets on $REPO"
base64 < "$OUT/developer-id.p12" | tr -d '\n' | gh secret set MACOS_CERT_P12 --repo "$REPO"
printf '%s' "$P12_PASS" | gh secret set MACOS_CERT_PASSWORD --repo "$REPO"
base64 < "$P8" | tr -d '\n' | gh secret set APPLE_API_KEY --repo "$REPO"
printf '%s' "$KEY_ID" | gh secret set APPLE_API_KEY_ID --repo "$REPO"
printf '%s' "$ISSUER" | gh secret set APPLE_API_ISSUER_ID --repo "$REPO"

echo
echo "Done. The next release will be signed and notarized."
echo "Your certificate and its private key are in $OUT — back that folder up"
echo "somewhere safe (a password manager is ideal), it cannot be downloaded again."
