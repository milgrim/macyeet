# macyeet

Yeet files into any app via native macOS drag and drop.

![macyeet screenshot](macyeet.png)

macyeet uses native AppKit APIs (`NSPasteboard` with `public.file-url`), so dragged files are recognized as real file drops by Electron apps like Microsoft Teams, Slack, Discord, and others.

## Install

```
cargo install macyeet
```

Or build from source:

```
git clone https://github.com/milgrim/macyeet
cd macyeet
cargo install --path .
```

## Usage

```
yeet file1.txt file2.pdf
yeet --and-exit *.png
yeet -x $(find . -name '*.log')
```

The window shows the files with size and modification time. Click and drag from the window into any application.

Press `q` or `Escape` to quit. Use `-x` / `--and-exit` to automatically quit after a successful drag.

## Integration with yazi

Add to your `~/.config/yazi/keymap.toml`:

```toml
[[mgr.prepend_keymap]]
on = "<C-d>"
run = 'shell -- yeet --and-exit "$@"'
desc = "Yeet selected files (native drag and drop)"
```

## Requirements

- macOS (this is macOS-only by design)
- Rust toolchain for building

## License

MIT

---

Built with [Claude Code](https://claude.ai/code)
