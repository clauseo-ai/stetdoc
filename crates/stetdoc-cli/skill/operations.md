# stetdoc operations

Every operation of stetdoc {{version}}, with its fields and an example in block text. A batch starts with `base:` and the
token, and each operation starts with `---` and `@name`. A field is `key: value` on one line, or `key: <<END` with lines up to
a line that is exactly `END`. Required fields are marked. JSON works too: `{"base": "...", "ops": [{"op": "replace", ...}]}`.

{{ops_reference}}

## Limits

{{limits}}
