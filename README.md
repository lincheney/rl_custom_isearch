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

Copy [src/rl_custom_isearch](src/rl_custom_isearch)
in to your `$PATH` (or write your own).
The provided script requires [fzf](https://github.com/junegunn/fzf)

Add a binding to your `~/.inpurc`:
```
"\C-r": librl_custom_isearch
"\C-s": librl_custom_isearch
```

Run:
```bash
LD_PRELOAD=/path/to/librl_custom_function.so \
READLINE_CUSTOM_FUNCTION_LIBS=./target/release/librl_custom_isearch.so \
python
```

Hit control-r
