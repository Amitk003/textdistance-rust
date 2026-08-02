"""Lone-surrogate handling in the edit, sequence, simple, and phonetic families.

Python strings are sequences of code points, and a lone surrogate (U+D800..DFFF)
is a valid one-element code point even though it is not valid Unicode text. The
original's edit/sequence/simple/phonetic algorithms operate on those code points,
so the port must compute the same result instead of raising. These expected
values are pinned from the upstream reference at the recorded commit.

The str fast paths in the extension now extract code points directly (with a
Python-semantics fallback when UTF-8 would reject a surrogate), so a lone
surrogate flows through the kernels like any other character, matching the
original bit for bit. The interesting consequence: a surrogate is a distinct
unit, so it compares unequal to every ASCII character, exactly as upstream.
"""

import textdistance as td

import pytest

S = '\ud800'


def test_levenshtein_surrogates():
    assert td.levenshtein('a' + S, S + 'a') == 2
    assert td.levenshtein(S, S) == 0
    assert td.levenshtein('a' + S, S + 'ba') == 3
    assert td.levenshtein(S + 'x', 'x' + S) == 2
    assert type(td.levenshtein(S, S)) is int


def test_damerau_levenshtein_surrogates():
    assert td.damerau_levenshtein('a' + S, S + 'a') == 1
    assert td.damerau_levenshtein(S, S) == 0
    assert td.damerau_levenshtein(S + 'x', 'x' + S) == 1
    assert td.damerau_levenshtein('ax', 'xb') == 2


def test_hamming_surrogates():
    assert td.hamming('a' + S, S + 'a') == 2
    assert td.hamming(S, S) == 0
    assert td.hamming('a' + S, S + 'ba') == 3


def test_jaro_surrogates():
    assert td.jaro(S, S) == 1
    assert td.jaro('a' + S, S + 'a') == pytest.approx(0.0)


def test_mlipns_surrogates():
    # The original MLIPNS returns 1 or 0, as an int, always.
    for a, b in [('a' + S, S + 'a'), (S, S), ('a' + S, S + 'ba'), (S + 'x', 'x' + S)]:
        value = td.mlipns(a, b)
        assert type(value) is int
        assert value in (0, 1)


def test_needleman_wunsch_surrogates():
    assert td.needleman_wunsch(S, S) == pytest.approx(1.0)
    assert td.needleman_wunsch('a' + S, S + 'a') == pytest.approx(0.0)


def test_smith_waterman_surrogates():
    assert td.smith_waterman(S, S) == 1
    assert td.smith_waterman('a' + S, S + 'a') == pytest.approx(0.0)


def test_gotoh_surrogates():
    assert td.gotoh(S, S) == pytest.approx(1.0)
    assert td.gotoh('a' + S, S + 'a') == pytest.approx(0.0)


def test_editex_surrogates():
    # S is not in any group and not ungrouped, so every pairing is a mismatch.
    assert td.editex(S, S) == 0
    assert td.editex('a' + S, S + 'a') == 4
    assert td.editex('a' + S, S + 'ba') == 6


def test_lcsseq_surrogates():
    assert td.lcsseq('a' + S, S + 'a') == 'a'
    assert td.lcsseq(S, S) == S
    assert td.lcsseq(S + 'x', 'x' + S) == S


def test_lcsstr_surrogates():
    assert td.lcsstr(S, S) == S
    assert td.lcsstr('a' + S, S + 'a') == 'a'


def test_ratcliff_obershelp_surrogates():
    assert td.ratcliff_obershelp(S, S) == 1
    assert td.ratcliff_obershelp('a' + S, S + 'a') == pytest.approx(0.5)
    assert td.ratcliff_obershelp('a' + S, S + 'ba') == pytest.approx(0.4)


def test_mra_surrogates():
    assert td.mra(S, S) == 1
    assert td.mra('a' + S, S + 'a') == 0


def test_prefix_postfix_surrogates():
    assert td.prefix(S, S) == S
    assert td.prefix('a' + S, S + 'a') == ''
    assert td.postfix(S, S) == S
    assert td.postfix('a' + S, 'a' + S) == 'a' + S