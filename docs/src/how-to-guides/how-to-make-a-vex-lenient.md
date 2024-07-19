# How to make a vex lenient

1. Open the `.star` file containing the vex
2. Put the cursor at the start of the `open_project` or `open_file` handler function (not the `vex.observe` line)
3. Type the following—
    ```python
    if vex.lenient:
        return
    ```
