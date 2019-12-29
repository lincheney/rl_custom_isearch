# rl_custom_isearch

Hack to customise readline history search.

History search is delegated to a script instead,
which receives history lines on stdin and outputs
the selected line on stdout.

This relies on https://github.com/lincheney/rl_custom_function

## How to use

Build the library:
```bash
cargo build --release
```

You should now have a .so at `./target/release/librl_custom_isearch.so`

Copy [bin/rl_custom_isearch](bin/rl_custom_isearch)
in to your `$PATH` (or write your own).
The provided script requires [fzf](https://github.com/junegunn/fzf)

Add to your `~/.inputrc`:
```
$include function rl_custom_isearch /path/to/librl_custom_isearch.so
"\C-r": rl_custom_isearch
"\C-s": rl_custom_isearch
```

Run something interactive that uses readline, e.g. python:
```bash
LD_PRELOAD=/path/to/librl_custom_function.so python
```

... and press control-r.
