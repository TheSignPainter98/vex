# Globs

Globs are patterns, commonly seen in shell language, used to represent one or multiple file paths.

Vex supports the following glob features:[^glob-citation]

- `?` matches any single character.
- `*` matches any (possibly empty) sequence of characters.
- `**` matches the current directory and arbitrary subdirectories. This sequence must form a single path component, so both `**a` and `b**` are invalid and will result in an error. A sequence of more than two consecutive `*` characters is also invalid.
- `[...]` matches any character inside the brackets. Character sequences can also specify ranges of characters, as ordered by Unicode, so e.g. `[0-9]` specifies any character between 0 and 9 inclusive. An unclosed bracket is invalid.
- `[!...]` is the negation of `[...]`, i.e. it matches any characters not in the brackets.
- The metacharacters `?`, `*`, `[`, `]` can be matched by using brackets (e.g. `[?]`). When a `]` occurs immediately following `[` or `[!` then it is interpreted as being part of, rather then ending, the character set, so `]` and NOT `]` can be matched by `[]]` and `[!]]` respectively. The `-` character can be specified inside a character sequence pattern by placing it at the start or the end, e.g. `[abc-]`.

[^glob-citation]: This list originally came from the [`Pattern` docs](https://docs.rs/glob/latest/glob/struct.Pattern.html) in the excellent [`glob` Rust crate](https://docs.rs/glob/latest/glob/index.html), used by this project.
