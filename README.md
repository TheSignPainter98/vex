# `vex`

_A blazingly-fast, hackable linter_

Every PR should seek to be a good PR, meaning that it is both _functionally_ and _stylistically_ correct.
Whereas correct functionality can be ensured with good design and tests, style is a more human affair and hence is much harder to pin down.
Even if we assume correctly formatted code and an absence of lint, there are still plenty of ways in which one solution may differ from another.
As a result, when working conventions naturally arise (especially on large code-bases), enforced only by fallible human review.
So what if we were able to express some of these idiosyncratic style decisions in a form amenable to automation?
This is where `vex` comes in.

`vex` is a hackable linter, intended to act as an enforcer for subjective, project-local style preferences.
Taking input of a set of [Starlark][starlark] scripts which express style rules, it scans the project directories to find style problems.

`vex` supports Linux, macos and Windows, as well as a variety of languages including Rust, Go, C/C++ and Java. (For a complete up-to-date list, run `vex languages` once installed.)

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
First, let’s tell `vex` that when opening the project at the start of its run, we intend to—in this example—find binary expressions.
```python
def init():
    vex.observe('open_project', on_open_project)

def on_open_project(event):
    vex.search(
        'rust',
        '''
            (binary_expression
                left: (integer_literal) @left_operand
                right: (integer_literal) @right_operand
            ) @bin_expr
        ''',
```
Note that in this [Scheme][scheme] query, we have labelled certain nodes for later use: `@left_operand`, `@right-operand` and `@bin_expr`.
(For a given file, to see the syntax tree these queries are performed against, run `vex dump path/to/file`.)

To react to a syntax-tree node being found which matches the above query, add a callback function (we’ll call this `on_match`)—
```python
        on_match,
    )
```

Now, let’s fill in that `on_match` function.
Let’s say we want to enforce that every time two integer literals appear in a binary expression, a significantly smaller one should appear first (perhaps so the reader isn’t too distracted by the large number that they neglect to read the smaller one).
To do this, write our `on_match` function as follows.
```python
def on_match(event):
    left_operand = event.captures['left_operand']
    right_operand = event.captures['right_operand']
    bin_expr = event.captures['bin_expr']
    if int(str(left_operand)) >= int(str(right_operand)) / 1000:
        vex.warn(
            'large operands should come later',
            at=(left_operand, 'number too large'),
            info='large numbers can hide smaller ones',
        )
```
Et voilà, you now have a lint!
To see it in action, create a new file like this, somewhere in your project (but outside of the `vexes/` directory!)—
```rust
fn func() -> i32 {
    123456 + 1
}
```
then type—
```bash
vex check
```
and see the pretty output as `vex` notifies you that the left operand is larger than the right.

## Author and License

This project is maintained by Ed Jones and is licensed under the GNU General Public License version 3.

[scheme]: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
[starlark]: https://github.com/bazelbuild/starlark/blob/master/spec.md
