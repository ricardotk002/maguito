# maguito

A fast git TUI inspired by Magit

![maguito screenshot](docs/screenshot.png)

## Install

```bash
cargo install --path .
```

## Usage

Run from any git repository:

```bash
maguito
```

Or bind it in `~/.tmux.conf`:

```
bind-key g split-window -h -c "#{pane_current_path}" "maguito"
```

## Keys

| Key       | Action                                    |
| --------- | ----------------------------------------- |
| `j` / `k` | Move down / up                            |
| `Tab`     | Expand or collapse section, file, or hunk |
| `s`       | Stage file or hunk under cursor           |
| `u`       | Unstage file or hunk under cursor         |
| `c c`     | Commit staged changes                     |
| `g`       | Refresh                                   |
| `q`       | Quit                                      |
