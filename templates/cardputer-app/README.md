# {{project-name}}

A Cardputer ADV firmware on [oxstack](https://github.com/theashraf/oxstack).

```sh
cargo install espup espflash --locked   # once
espup install                           # Xtensa Rust toolchain → ~/export-esp.sh
source ~/export-esp.sh                  # every new shell
cargo run --release                     # build, flash, open the defmt monitor
```

API docs: <https://theashraf.github.io/oxstack/oxstack_cardputer/>.
