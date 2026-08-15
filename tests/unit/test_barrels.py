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
