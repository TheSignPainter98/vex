# `vex`

Every PR should seek to be a good PR, meaning that it is both _functionally_ and _stylistically_ correct.
Whereas correct functionality can be ensured with good design and tests, style is a more human affair and hence is harder to pin down.
Although a baseline of common style may be achieved by requiring correct formatting and an absence of linting errors, such opinions often do not go far enough.
When working on large code-bases, conventions naturally arise, which must be enforced by human reviewers, however after a certain point, no one human can have be reasonably expected to reliably police all changes.
As a baseline, we should assume that code must be correctly formatted and that no lints are reported from the linter of the relevant language, however when working on large code-bases, conventions naturally start to arise and must be enforced by human reviewers.
Therefore, automation is required and this is where `vex` comes in.

`vex` is a hackable linter, intended to act as an enforcer for subjective, project-local style preferences.
Taking input of a set of [Starlark][starlark] scripts which express the style rules, it scans the source directory to identify and report problems.

## Installation

To install `vex`, type and run—
```bash
git clone https://github.com/TheSignPainter98/vex
cd vex/
cargo install --path .
```
Ensure that `~/.cargo/bin` is in your path and test this by typing—
```bash
vex --version
```

## How to use

Once installed, to start using `vex` in your project, `cd` to the project’s root and type
```bash
vex init
```

To create a lint, in the newly-created `vexes/` directory create a new file called `<my_lint_name>.star`.
Open that file in your editor and let’s set up your script.
First, let’s add some triggers to tell `vex` when to run your script.
To add a new trigger which finds binary expressions between integers in all Rust files in the project, type the following—
```python
def init():
    vex.add_trigger(
        language='rust',
        query='''
            (binary_expression
                left: (integer_literal) @left_operand
                right: (integer_literal) @right_operand
            ) @bin_expr
        ''',
    )
```
Note that in this [Scheme][scheme] query, we have labelled certain nodes we want to find (`@left_operand`, `@right-operand` and `@bin_expr`), we will use these later.

To react to this query matching something, add an observer for the `query_match` event (we’ll call this `on_query_match`) by typing the following in `init`—
```python
    vex.observe('query_match', on_query_match)
```

Now, let’s fill in that observer.
Let’s say we want to enforce that every time two integer literals appear in a binary expression, a significantly smaller one should appear first (perhaps so the reader isn’t too distracted by the large number that they neglect to read the smaller one).
To do this, write our `on_query_match` function as follows.
```bash
def on_query_match(event):
    left_operand = event.captures['left_operand']
    right_operand = event.captures['right_operand']
    bin_expr = event.captures['bin_expr']
    if int(left_operand.text()) <= int(right_operand.text()) / 100
        vex.warn(
            ‘large numbers should come later’,
            at=bin_expr,
            see_also=[
                (left_operand, ‘left’),
                (right_operand, ‘is larger than right’)
            ],
        )
```
Et voilà! You now have a lint! To see it in action, type
```bash
vex check
```
To see it in action, create a new file like this, somewhere in your project (but outside of the `vexes/` directory!)—
```rust
fn func() -> i32 {
    4632784632 + 1
}
```

## Author and License

This project is maintained by Ed Jones and is licensed under the GNU General Public License version 3.

[scheme]: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
[starlark]: https://github.com/bazelbuild/starlark/blob/master/spec.md
