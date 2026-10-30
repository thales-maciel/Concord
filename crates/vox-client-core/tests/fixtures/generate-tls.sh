#!/usr/bin/env bash
# Public, test-only credentials for the loopback TLS regression test. Never use outside tests.
set -euo pipefail
cd "$(dirname "$0")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout "$tmp/ca.key" -out "$tmp/ca.pem" -days 36500 -subj '/CN=Concord test CA' \
  -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign'
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout "$tmp/localhost.key" -out "$tmp/localhost.csr" -subj '/CN=localhost'
printf '%s\n' 'basicConstraints=critical,CA:FALSE' 'keyUsage=critical,digitalSignature' \
  'extendedKeyUsage=serverAuth' 'subjectAltName=DNS:localhost' > "$tmp/extensions"
openssl x509 -req -in "$tmp/localhost.csr" -CA "$tmp/ca.pem" -CAkey "$tmp/ca.key" \
  -CAcreateserial -out "$tmp/localhost.pem" -days 36500 -extfile "$tmp/extensions"
openssl x509 -in "$tmp/ca.pem" -outform DER -out tls-ca.der
openssl x509 -in "$tmp/localhost.pem" -outform DER -out tls-localhost.der
openssl pkcs8 -topk8 -nocrypt -in "$tmp/localhost.key" -outform DER -out tls-localhost-key.der
