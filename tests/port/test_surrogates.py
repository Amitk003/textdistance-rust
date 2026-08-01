"""Lone-surrogate handling in the compression family.

Python strings are sequences of code points, and a lone surrogate (U+D800..DFFF)
is a valid one-element code point even though it is not valid Unicode text. The
pure compression coders must therefore accept such strings and match the
original bit for bit. The expected values below are pinned from the upstream
reference at the recorded commit.

The binary compressors encode to UTF-8 first, exactly like the original, so a
lone surrogate raises UnicodeEncodeError on both sides; that failure-mode parity
is asserted too.
"""

import textdistance as td

import pytest

S = '\ud800'


def test_entropy_ncd_surrogates():
    assert td.entropy_ncd('a' + S, 'a') == pytest.approx(0.4591479170272448)
    assert td.entropy_ncd(S, S) == 0.0
    assert td.entropy_ncd._get_size('0' + S) == pytest.approx(2.0)


def test_sqrt_ncd_surrogates():
    assert td.sqrt_ncd('a' + S, 'a') == pytest.approx(0.7071067811865475)
    assert td.sqrt_ncd(S, S) == pytest.approx(0.41421356237309515)
    assert td.sqrt_ncd._get_size('0' + S) == pytest.approx(2.0)


def test_rle_ncd_surrogates():
    assert td.rle_ncd('a' + S, 'a') == pytest.approx(1.0)
    assert td.rle_ncd._compress('aaab' + S) == '3ab' + S


def test_bwtrle_ncd_surrogates():
    assert td.bwtrle_ncd('a' + S, 'a') == pytest.approx(0.6666666666666666)
    assert td.bwtrle_ncd._compress('ab' + S + 'c') == 'c\x00a' + S + 'b'


def test_arith_ncd_surrogates():
    assert td.arith_ncd('a' + S, 'a') == pytest.approx(0.0)


def test_binary_ncds_raise_on_surrogates_like_the_original():
    for alg in (td.bz2_ncd, td.zlib_ncd, td.lzma_ncd):
        with pytest.raises(UnicodeEncodeError):
            alg('a' + S, 'a')
