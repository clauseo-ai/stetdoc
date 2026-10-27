#!/bin/sh
# stetdoc's guard for Claude Code: a PreToolUse hook that stops Claude from reading or writing a Word package (.docx,
# .docm, .dotx, .dotm) by other means than stetdoc, such as unzipping it and rewriting word/document.xml with a script.
# Such an edit is not a tracked change, and it can break the package.
#
# Claude Code passes the tool call as JSON on stdin. The script prints nothing and exits 0 to allow it, or explains on
# stderr and exits 2 to block it. It needs only sh and awk, never stetdoc, because the case it guards against is the
# one where stetdoc is missing. It allows every `stetdoc` command, and when it is unsure it allows: a missed command is
# better than a blocked stetdoc task. docs/agents.md lists what it blocks. STETDOC_GUARD=off in Claude Code's
# environment turns it off.

case "${STETDOC_GUARD:-}" in
off | 0 | false | no) exit 0 ;;
esac

prog='
# Decodes a the escapes of a JSON string. The input has `\\` as \001 and `\"` as \002 already.
function unescape(s,   out, v, h) {
    gsub(/\\n/, "\n", s); gsub(/\\t/, "\t", s); gsub(/\\r/, "", s); gsub(/\\[bf]/, " ", s); gsub(/\\\//, "/", s)
    out = ""
    while (match(s, /\\u[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/)) {
        h = tolower(substr(s, RSTART + 2, 4))
        v = 0
        for (k = 1; k <= 4; k++) v = v * 16 + index("0123456789abcdef", substr(h, k, 1)) - 1
        out = out substr(s, 1, RSTART - 1) ((v > 31 && v < 127) ? sprintf("%c", v) : (v == 10 ? "\n" : "?"))
        s = substr(s, RSTART + 6)
    }
    s = out s
    gsub("\002", "\"", s); gsub("\001", "\\", s)
    return s
}

# Sets J[key] to the first string value of each key. Splitting on `"` once keeps it linear in the input\047s size.
function json(s,   n, k, p) {
    gsub(/\\\\/, "\001", s); gsub(/\\"/, "\002", s)
    n = split(s, p, "\"")
    for (k = 2; k + 2 <= n; k += 2)
        if (p[k + 1] ~ /^[ \t\r\n]*:[ \t\r\n]*$/ && !(p[k] in J)) J[p[k]] = unescape(p[k + 2])
}

function trim(s) { sub(/^[ \t\n]+/, "", s); sub(/[ \t\n]+$/, "", s); return s }

# Cuts a shell command into simple commands at unquoted ; & | ( ) ` and newlines, drops comments, and gives each
# here-document body to the command it belongs to, in BODY. It is not a shell parser, only close enough to tell
# which command each word belongs to.
function cut(c,   n, i, ch, start, quote, nh, h, j, k, d, q, strip, bstart, e, line) {
    n = length(c); i = 1; start = 1; nseg = 0; nh = 0; quote = ""
    while (i <= n) {
        ch = substr(c, i, 1)
        if (quote == "\047") { if (ch == "\047") quote = ""; i++; continue }
        if (quote == "\"") {
            if (ch == "\\") { i += 2; continue }
            if (ch == "\"") quote = ""
            i++; continue
        }
        if (ch == "\\") { i += 2; continue }
        if (ch == "\047" || ch == "\"") { quote = ch; i++; continue }
        if (ch == "#" && (i == 1 || index(" \t\n;&|()", substr(c, i - 1, 1)))) {
            SEG[++nseg] = substr(c, start, i - start)
            while (i <= n && substr(c, i, 1) != "\n") i++
            start = i; continue
        }
        if (ch == "<" && substr(c, i + 1, 1) == "<" && substr(c, i + 2, 1) != "<") {
            j = i + 2; strip = 0
            if (substr(c, j, 1) == "-") { j++; strip = 1 }
            while (substr(c, j, 1) == " " || substr(c, j, 1) == "\t") j++
            if (substr(c, j, 1) == "\\") j++
            q = substr(c, j, 1)
            if (q == "\047" || q == "\"") {
                j++; k = j
                while (j <= n && substr(c, j, 1) != q) j++
                d = substr(c, k, j - k); j++
            } else {
                k = j
                while (j <= n && substr(c, j, 1) ~ /[A-Za-z0-9_.-]/) j++
                d = substr(c, k, j - k)
            }
            if (d != "") { nh++; HD[nh] = d; HS[nh] = strip; HO[nh] = nseg + 1 }
            i = j; continue
        }
        if (index("\n;&|()`", ch)) {
            SEG[++nseg] = substr(c, start, i - start)
            i++; start = i
            if (ch == "\n" && nh > 0) {
                for (h = 1; h <= nh; h++) {
                    bstart = i; e = i
                    while (i <= n) {
                        e = i
                        while (e <= n && substr(c, e, 1) != "\n") e++
                        line = substr(c, i, e - i)
                        if (HS[h]) sub(/^\t+/, "", line)
                        if (line == HD[h]) break
                        i = e + 1
                    }
                    BODY[HO[h]] = BODY[HO[h]] substr(c, bstart, i - bstart) "\n"
                    i = e + 1
                }
                nh = 0; start = i
            }
            continue
        }
        i++
    }
    SEG[++nseg] = substr(c, start)
}

# The program a simple command runs: its first word after variable assignments, keywords and wrappers such as env,
# sudo and xargs, without its directory or quotes.
function lead(s,   w, n, k, t) {
    n = split(s, w, /[ \t\n]+/)
    for (k = 1; k <= n; k++) {
        t = w[k]
        gsub(/["\047]/, "", t)
        if (t == "" || t ~ /^[A-Za-z_][A-Za-z0-9_]*=/ || t ~ /^-/) continue
        if (t ~ /^(if|then|else|elif|do|while|until|!|\{|\}|time|env|command|exec|builtin|nohup|nice|sudo|xargs|busybox|npx|bunx|dlx)$/) continue
        if (t == "pnpm" || t == "yarn") continue
        sub(/.*\//, "", t)
        sub(/@.*/, "", t)
        return t
    }
    return ""
}

# Whether a program from the alternation re runs in segment s: as its first word, or right after a quote (`sh -c
# "unzip ..."`), `-exec` or a wrapper.
function runs(s, re,   pre) {
    pre = "(^|\n|[\"\047]|-exec[ \t]+|xargs([ \t]+-[^ \t]+)*[ \t]+|sudo[ \t]+|env[ \t]+|nohup[ \t]+|time[ \t]+|exec[ \t]+|command[ \t]+|busybox[ \t]+)"
    return s ~ (pre "[ \t]*([^ \t\"\047]*/)?(" re ")([ \t\"\047]|$)")
}

function verdict(why) { print "block:" why; exit }

# Raw newlines are only whitespace in JSON, and some awks also split on them below, so they become spaces.
{ input = input $0 " " }

END {
    json(input)
    tool = J["tool_name"]
    pkg = "[.](docx|docm|dotx|dotm)([^a-z0-9]|$)"
    part = "(word/[A-Za-z0-9_.]*[.]xml|document[.]xml|[[]?content_types[]]?[.]xml)"
    doclib = "(import[ \t]+docx([ \t\n,;.\"\047]|$)|from[ \t]+docx([.]|[ \t]+import)|python-docx|docx2python|docx2txt|docxtpl|docxcompose|docx2pdf|mammoth|require[(][\"\047]docx[\"\047][)]|from[ \t]+[\"\047]docx[\"\047]|docxtemplater|officegen|docx4js|docx4j|XWPFDocument|DocumentFormat[.]OpenXml|WordprocessingDocument|Aspose[.]Words|aspose-words|spire[.]doc|Docx::Document|require[ \t]+[\"\047]docx[\"\047]|(npm|pnpm|yarn|bun)[ \t]+(install|i|add)[ \t][^;&|]*docx([ \t\n@]|$))"
    ziplib = "(zipfile|ZipFile|ZipArchive|JSZip|jszip|AdmZip|adm-zip|yauzl|unzipper|PizZip|pizzip|Archive::Zip|Zip::File|IO::Uncompress::Unzip|java[.]util[.]zip|archive/zip|System[.]IO[.]Compression|require[ \t]+[\"\047]zip[\"\047])"
    archive = "unzip|zip|zipinfo|zipnote|zipsplit|zipcloak|zipgrep|zipdetails|funzip|ditto|tar|bsdtar|gtar|7z|7za|7zr|7zz|jar|unar|lsar|Expand-Archive|Compress-Archive"
    convert = "textutil|pandoc|soffice|libreoffice|lowriter|unoconv|docx2txt|docx2pdf|antiword|catdoc|abiword"
    interp = "^(python[0-9.]*|py|pypy[0-9]*|node|nodejs|deno|bun|ruby|irb|perl|php|osascript|Rscript|pwsh|powershell|java|jshell|swift|lua|tclsh)$"

    if (tool ~ /^(Write|Edit|MultiEdit|NotebookEdit)$/) {
        path = J["file_path"]; if (path == "") path = J["notebook_path"]
        lpath = tolower(path)
        if (lpath ~ ("[.](docx|docm|dotx|dotm)$")) verdict("a Word package is a zip file, and this tool would write it as text")
        if (lpath ~ ("(^|/)" part "$")) verdict("this file looks like a part unzipped from a Word package")
        # A script written to a temporary folder to take a Word package apart, as in the failure this guard exists for.
        if (lpath ~ /^(\/private)?(\/tmp|\/var\/folders)\// && lpath ~ /[.](py|js|mjs|cjs|ts|rb|pl|php|sh)$/) {
            body = J["content"] J["new_string"] J["new_source"]
            if (body ~ doclib || (body ~ ziplib && tolower(body) ~ pkg)) verdict("this script reads or writes a Word package without stetdoc")
        }
        exit
    }
    if (tool != "Bash" && tool != "PowerShell") exit

    cut(J["command"])
    text = ""
    for (s = 1; s <= nseg; s++) {
        seg = trim(SEG[s]); L[s] = lead(seg)
        # The stetdoc commands are always allowed, here-document included. Commit and pull request messages are prose.
        if (L[s] == "stetdoc" || L[s] == "git" || L[s] == "gh") { SKIP[s] = 1; continue }
        T[s] = seg "\n" BODY[s]
        text = text "\n" T[s]
    }
    ltext = tolower(text)
    haspkg = ltext ~ pkg
    if (text ~ doclib) verdict("it uses a library that reads or writes .docx files directly")
    for (s = 1; s <= nseg; s++) {
        if (SKIP[s]) continue
        seg = T[s]; lseg = tolower(seg); l = L[s]
        if (haspkg && runs(seg, archive)) verdict("it opens a Word package with an archive tool")
        if (haspkg && runs(seg, convert)) verdict("it converts a Word package with another tool")
        if (l ~ interp && lseg ~ pkg) verdict("it runs a script on a Word package")
        if (lseg ~ pkg && lseg ~ /[.]zip([^a-z0-9]|$)/) verdict("it turns a Word package into a .zip file")
        if (l ~ /^(sed|gsed|perl|awk|gawk)$/ && seg ~ /[ \t]-(i|pi|[a-z]*i)/ && (lseg ~ pkg || lseg ~ part)) verdict("it edits a Word package or its parts in place")
        if (l ~ /^(dd|truncate|xxd)$/ && lseg ~ pkg) verdict("it writes bytes into a Word package")
        if (lseg ~ part && (l ~ interp || l ~ /^(sed|gsed|awk|gawk|xmllint|xmlstarlet|xml|xsltproc|cat|head|tail|less|more|tee)$/)) verdict("it reads or writes a part unzipped from a Word package")
        if (lseg ~ (">[ \t]*[^ \t]*" part)) verdict("it writes a part of a Word package")
    }
    if (haspkg && text ~ ziplib) verdict("it reads or writes a Word package as a zip file")
}
'

verdict=$(awk "$prog") || exit 0
case "$verdict" in
block:*)
    cat >&2 <<EOF
stetdoc's guard blocked this: ${verdict#block:}. A Word file (.docx) is edited only through stetdoc, which records
every change as a Word tracked change and leaves the rest of the file untouched. Find text with
\`stetdoc view <file> --find TEXT\`, change it with \`stetdoc replace <file> OLD NEW -o OUT\`, comment with
\`stetdoc comment\`, and look underneath with \`stetdoc inspect\` (\`stetdoc skills get core\` has the rest). If
\`stetdoc\` is not found, tell the user that stetdoc is not installed (https://github.com/clauseo-ai/stetdoc) and stop.
Do not try another way around this.
EOF
    exit 2
    ;;
esac
exit 0
