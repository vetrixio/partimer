#!/usr/bin/env bash
# Print the CHANGELOG.md section of one version as release notes: the lines under
# "## v<version> - <date>" up to the next "## v" heading, with "###" headings raised to "##"
# and leading blank lines dropped. Fails when the section is missing or empty.
#
#   scripts/changelog-section.sh 0.1.3 [CHANGELOG.md]
set -euo pipefail

version="${1:?usage: changelog-section.sh <version> [changelog]}"
file="${2:-CHANGELOG.md}"

notes="$(awk -v head="## v${version}" '
  index($0, head) == 1 && (length($0) == length(head) || substr($0, length(head) + 1, 1) == " ") { found = 1; next }
  /^## v/ { if (found) exit }
  found { print }
' "$file" | sed 's/^### /## /' | sed '/./,$!d')"

if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "no CHANGELOG section for v${version} in ${file}" >&2
  exit 1
fi
printf '%s\n' "$notes"
