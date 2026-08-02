"""Surface parity with the upstream textdistance 4.6.2 public API.

The expected name sets below were captured from the pinned upstream package
(commit d6a68d61088a40eef5c88191ccf79323dbf34850) by listing the public
attributes of the top-level module and of ``textdistance.utils``.

These tests prove that the port exposes exactly the same names as the
original, no more and no fewer. Any drift in the exported surface fails here.
"""

import textdistance as td
import textdistance.utils as utils

import pytest

UPSTREAM_VERSION = "4.6.2"

EXPECTED_TD = [
    "ArithNCD",
    "BWTRLENCD",
    "BZ2NCD",
    "Bag",
    "Cosine",
    "DamerauLevenshtein",
    "Editex",
    "EntropyNCD",
    "Gotoh",
    "Hamming",
    "Identity",
    "Jaccard",
    "Jaro",
    "JaroWinkler",
    "LCSSeq",
    "LCSStr",
    "LZMANCD",
    "Length",
    "Levenshtein",
    "MLIPNS",
    "MRA",
    "Matrix",
    "MongeElkan",
    "NeedlemanWunsch",
    "Overlap",
    "Postfix",
    "Prefix",
    "RLENCD",
    "RatcliffObershelp",
    "SmithWaterman",
    "Sorensen",
    "SqrtNCD",
    "StrCmp95",
    "Tanimoto",
    "Tversky",
    "VERSION",
    "ZLIBNCD",
    "algorithms",
    "arith_ncd",
    "bag",
    "base",
    "bwtrle_ncd",
    "bz2_ncd",
    "compression_based",
    "cosine",
    "damerau_levenshtein",
    "dice",
    "edit_based",
    "editex",
    "entropy_ncd",
    "find_ngrams",
    "gotoh",
    "hamming",
    "identity",
    "jaccard",
    "jaro",
    "jaro_winkler",
    "lcsseq",
    "lcsstr",
    "length",
    "levenshtein",
    "libraries",
    "lzma_ncd",
    "matrix",
    "mlipns",
    "monge_elkan",
    "mra",
    "needleman_wunsch",
    "overlap",
    "phonetic",
    "postfix",
    "prefix",
    "ratcliff_obershelp",
    "rle_ncd",
    "sequence_based",
    "simple",
    "smith_waterman",
    "sorensen",
    "sorensen_dice",
    "sqrt_ncd",
    "strcmp95",
    "tanimoto",
    "token_based",
    "tversky",
    "types",
    "utils",
    "words_combinations",
    "zlib_ncd",
]

EXPECTED_UTILS = [
    "Sequence",
    "annotations",
    "find_ngrams",
    "permutations",
    "product",
    "words_combinations",
]


def public_names(obj):
    return sorted(n for n in dir(obj) if not n.startswith("_"))


def test_version():
    assert td.__version__ == UPSTREAM_VERSION
    assert td.VERSION == UPSTREAM_VERSION


def test_top_level_surface():
    assert public_names(td) == EXPECTED_TD


def test_utils_surface():
    assert public_names(utils) == EXPECTED_UTILS


def test_singletons_are_instances():
    for name in (
        "arith_ncd", "bwtrle_ncd", "bz2_ncd", "cosine", "damerau_levenshtein",
        "editex", "entropy_ncd", "gotoh", "hamming", "identity", "jaccard",
        "jaro", "jaro_winkler", "lcsseq", "lcsstr", "length", "levenshtein",
        "matrix", "mlipns", "monge_elkan", "mra", "needleman_wunsch",
        "overlap", "postfix", "prefix", "ratcliff_obershelp", "rle_ncd",
        "smith_waterman", "sorensen", "sorensen_dice", "sqrt_ncd", "strcmp95",
        "tanimoto", "tversky", "zlib_ncd",
    ):
        assert hasattr(td, name), name
        assert callable(getattr(td, name)), name


def test_gotoh_single_empty_matches_upstream_crash():
    """Faithful parity for the upstream gotoh bug (DECISIONS D23).

    Upstream 4.6.2 raises IndexError from its numpy DP when EXACTLY ONE of the
    two inputs is empty (it indexes row/col 1 of a length-1 axis). The port
    reproduces this exactly: distance/similarity/__call__ raise IndexError
    while normalized_distance/similarity short-circuit via maximum==0.
    """
    g = td.gotoh
    for s1, s2 in (("", "abc"), ("abc", ""), ("ab", "")):
        with pytest.raises(IndexError):
            g(s1, s2)
        with pytest.raises(IndexError):
            g.distance(s1, s2)
        with pytest.raises(IndexError):
            g.similarity(s1, s2)
        assert g.normalized_distance(s1, s2) == 0
        assert g.normalized_similarity(s1, s2) == 1


@pytest.mark.parametrize("s1,s2,nsim", [("", "", 1), ("abc", "def", 0.5), ("", "abc", 1)])
def test_gotoh_empty_both_or_full(s1, s2, nsim):
    g = td.gotoh
    if s1 == "" and s2 == "":
        assert g.distance(s1, s2) == -0.0
    assert g.normalized_similarity(s1, s2) == nsim
