import ast

import daf
import daf.core


def test_daf_is_subset_of_core():
    public = set(daf.__all__)
    core = set(daf.core.__all__)
    assert public.issubset(core), (
        f"daf.__all__ contains names not in daf.core.__all__: "
        f"{public - core}"
    )


def test_daf_names_are_importable():
    for name in daf.__all__:
        assert hasattr(daf, name), (
            f"`daf.{name}` is listed in __all__ but is not importable"
        )


def test_no_barrel_defines_own_public():
    import glob
    import os

    barrel_dir = os.path.dirname(daf.__file__)
    for init_path in glob.glob(os.path.join(barrel_dir, "*", "__init__.py")):
        with open(init_path) as f:
            source = f.read()
        tree = ast.parse(source)
        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef) and node.name == "_public":
                module_name = os.path.relpath(os.path.dirname(init_path), barrel_dir)
                raise AssertionError(
                    f"{module_name}/__init__.py defines its own _public(); "
                    f"it must import _public from daf._barrel"
                )
