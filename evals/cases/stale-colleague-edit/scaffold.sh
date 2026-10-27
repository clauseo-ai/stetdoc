#!/usr/bin/env bash
source "$(dirname "$0")/../../lib/scaffold.sh"
setup master-supply-agreement
# Turn 1, as the history file tells it: net 45 days, written to a new redline by ER.
v() { stetdoc view "$1" --find "$2" | sed -n '1s/.* base=\([^ ]*\).*/\1/p'; }
red=docs/master-supply-agreement.redline.docx
stetdoc edit docs/master-supply-agreement.docx -o "$red" >/dev/null <<EOF
base: $(v docs/master-supply-agreement.docx "thirty (30) days from")
---
@replace
ref: p145
find: thirty (30) days
with: forty-five (45) days
EOF
# Between the turns a colleague edits the same redline.
stetdoc edit "$red" --in-place --author "Dana Colleague" >/dev/null <<EOF
base: $(v "$red" "first-class international bank")
---
@replace
ref: p146
find: first-class international bank
with: first-class international bank rated at least A-
EOF
snapshot
