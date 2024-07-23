# How to support lint levels

Vex provides two lint levels: lenient and non-lenient.

## How to make a vex lenient

1. Open the file containing the `init` function which sets up the vex.
2. If the `open_project` event is observed, put the cursor at the top of its event handler function and type the following---
    ```python
    if vex.lenient:
        return
    ```
3. If the `open_file` event is observed, do the same to its event handler function.

## How to make a vex non-lenient

Vexes are non-lenient by default.
