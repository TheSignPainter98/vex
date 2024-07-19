# How to search child nodes

The approach to take is slightly different depending on whether a single matching child is required, or all matching children are required.

## How to search child nodes, returning the first match

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_child(node, predicate):
        def _find_child(node, predicate)
            for child in node.children():
                if predicate(child):
                    return child
                match = _find_child(child, predicate, ret=ret)
                if match != None:
                    return match
        ret = []
        _find_child(node, predicate, allow_nested, ret)
        return ret
    ```
3. Call the `find_child` function, passing the node whose children are to be searched and a function or lambda which takes a node returns whether it is the desired one—
    ```python
    let_declaration = find_child(node_to_search, lambda node: node.kind == 'let_declaration')
    ```
    The returned `let_declaration` is either a `Node` or `None`.

## How to search child nodes, returning all matches

1. Put the cursor at the end of vex script.
2. If absent, copy the following function into the script—
    ```python
    def find_children(node, predicate, allow_nested=False):
        def _find_children(node, predicate, allow_nested, ret)
            for child in node.children():
                if predicate(child):
                    ret.append(child)
                    if not allow_nested:
                        continue
                _find_children(child, predicate, allow_nested=allow_nested, ret=ret)
        ret = []
        _find_children(node, predicate, allow_nested, ret)
        return ret
    ```
3. Call the `find_children` function, passing the node whose children are to be searched and a function or lambda which takes a node returns whether to include it in the output, e.g.—
    ```python
    let_declarations = find_children(node_to_search, lambda node: node.kind == 'let_declaration', allow_nested=True)
    ```
    The returned `let_declaration` is a list.
