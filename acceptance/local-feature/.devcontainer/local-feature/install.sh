#!/bin/sh
set -eu

message="${MESSAGE:-local-feature-default}"

mkdir -p /usr/local/bin /usr/local/share/acceptance-local-feature

cat > /usr/local/bin/acceptance-local-feature <<EOF
#!/bin/sh
printf '%s\n' "${message}"
EOF

chmod +x /usr/local/bin/acceptance-local-feature
printf '%s\n' "${message}" > /usr/local/share/acceptance-local-feature/message
