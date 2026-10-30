# Usage

Install from this directory:

```sh
cargo install --path .
```

Run in the directory whose files you want to read:

```sh
max250k count
max250k count --exclude ./dir --exclude ./file.txt --config max250k.json
max250k read
max250k read --exclude ./dir --exclude ./file.txt --output ./out.txt --config max250k.json
max250k read > ../all-content.txt
```

`count` prints total lines, Unicode characters (including whitespace), and estimated
tokens using `tokenx_rs`. Counts are summed per file. Empty files have zero lines;
a final line without a newline still counts as one line. Token counts are estimates,
not model-specific exact counts.

`read` emits files in sorted path order, each preceded by
`# file content: <filepath>` using a path relative to the current directory.
A newline is inserted before the next header if the preceding content lacks one.
Use `--hide-filenames` to concatenate the original contents without any added
headers, separators, or newlines:

```sh
max250k read --hide-filenames
max250k read --hide-filenames --output ./out.txt
```

It writes to stdout unless `--output` is set. An existing
output file is overwritten and automatically excluded from the scan. When using
shell redirection, put the destination outside the scanned directory or explicitly
exclude it.

Both commands recursively scan the current directory, including hidden files and
files ignored by Git. Symlinks are not followed. Binary files (containing NUL bytes)
and non-UTF-8 files are skipped. Other read/traversal errors
cause a nonzero exit status.

Use repeated `--exclude` flags for exact file or directory paths, not glob patterns.
Excluding a directory excludes its descendants. For example:

```sh
max250k count --exclude .git --exclude target --exclude node_modules
```

Options are loaded automatically from `max250k.json` in the current directory
when it exists. Use `--config` to load a different JSON file instead:

```json
{
  "exclude": ["./dir", "./file.txt"],
  "output": "./out.txt",
  "log_level": "info",
  "hide_filenames": false
}
```

All keys are optional. `exclude` is an array of path strings; `output` is a path
string used only by `read`; `log_level` is `info` (default) or `debug`.
`hide_filenames` is a boolean (default `false`) used only by `read`.
`--hide-filenames` overrides it to `true`; `--hide-filenames=false` restores headers.
Unknown keys are rejected. All relative paths, including
those in JSON, are relative to the current directory, not the config's directory.
Config files are included in the scan unless explicitly excluded.

Command-line options override JSON options. Any CLI `--exclude` flags replace the
entire JSON exclude list rather than appending to it. CLI `--output` replaces JSON
`output`. Without `--config`, a missing `max250k.json` is ignored; an invalid or
unreadable default config causes an error. An explicitly specified config must exist.

At the default `info` level, `count` prints only `lines: N`, `characters: N`, and
`tokens: N`; `read` prints file contents with headers unless hidden (or writes them
to the output file). Counts always describe original file contents, excluding headers.
Use `debug` to log each successfully read file with its line, character, and
estimated token counts, and skipped paths with reasons to
stderr, without changing stdout. Excluded directories are logged once without
traversing their contents. Errors are reported on stderr at either level.
CLI `--log-level` overrides JSON `log_level`.

```sh
max250k count --log-level debug
max250k read --config max250k.json --log-level info
```

```sh
max250k --help
max250k count --help
max250k read --help
```
