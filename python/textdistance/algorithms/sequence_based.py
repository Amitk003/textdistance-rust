from __future__ import annotations

# built-in
from typing import Sequence

# app
from ..utils import find_ngrams
from .. import _textdistance
from .base import BaseSimilarity as _BaseSimilarity
from .types import TestFunc


__all__ = [
    'lcsseq', 'lcsstr', 'ratcliff_obershelp',
    'LCSSeq', 'LCSStr', 'RatcliffObershelp',
]


class LCSSeq(_BaseSimilarity):
    """longest common subsequence similarity

    https://en.wikipedia.org/wiki/Longest_common_subsequence_problem
    """

    def __init__(
        self,
        qval: int = 1,
        test_func: TestFunc = None,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.test_func = test_func or self._ident
        self.external = external

    def _dynamic(self, seq1: str, seq2: str) -> str:
        indices = _textdistance.lcsseq(seq1, seq2)
        result = ''
        for i in reversed(indices):
            result = seq1[i] + result
        return result

    def _recursive(self, *sequences: str) -> str:
        if not all(sequences):
            return type(sequences[0])()  # empty sequence
        if self.test_func(*[s[-1] for s in sequences]):
            c = sequences[0][-1]
            sequences = tuple(s[:-1] for s in sequences)
            return self(*sequences) + c
        m = type(sequences[0])()  # empty sequence
        for i, s in enumerate(sequences):
            ss = sequences[:i] + (s[:-1], ) + sequences[i + 1:]
            m = max([self(*ss), m], key=len)
        return m

    def __call__(self, *sequences: str) -> str:
        if not sequences:
            return ''
        sequences = self._get_sequences(*sequences)
        if len(sequences) == 2:
            return self._dynamic(*sequences)
        else:
            return self._recursive(*sequences)

    def similarity(self, *sequences) -> int:
        return len(self(*sequences))


class LCSStr(_BaseSimilarity):
    """longest common substring similarity
    """

    def _standart(self, s1: str, s2: str) -> str:
        besti, bestsize = _textdistance.lcsstr_standard(s1, s2)
        return s1[besti: besti + bestsize]

    def _custom(self, *sequences: str) -> str:
        short = min(sequences, key=len)
        length = len(short)
        for n in range(length, 0, -1):
            for subseq in find_ngrams(short, n):
                joined = ''.join(subseq)
                for seq in sequences:
                    if joined not in seq:
                        break
                else:
                    return joined
        return type(short)()  # empty sequence

    def __call__(self, *sequences: str) -> str:
        if not all(sequences):
            return ''
        length = len(sequences)
        if length == 0:
            return ''
        if length == 1:
            return sequences[0]

        sequences = self._get_sequences(*sequences)
        if length == 2 and max(map(len, sequences)) < 200:
            return self._standart(*sequences)
        return self._custom(*sequences)

    def similarity(self, *sequences: str) -> int:
        return len(self(*sequences))


class RatcliffObershelp(_BaseSimilarity):
    """Ratcliff-Obershelp similarity
    This follows the Ratcliff-Obershelp algorithm to derive a similarity
    measure:
        1. Find the length of the longest common substring in sequences.
        2. Recurse on the strings to the left & right of each this substring
           in sequences. The base case is a 0 length common substring, in which
           case, return 0. Otherwise, return the sum of the current longest
           common substring and the left & right recursed sums.
        3. Multiply this length by 2 and divide by the sum of the lengths of
           sequences.

    https://en.wikipedia.org/wiki/Gestalt_Pattern_Matching
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/ratcliff-obershelp.js
    https://xlinux.nist.gov/dads/HTML/ratcliffObershelp.html
    """

    def maximum(self, *sequences: str) -> int:
        return 1

    def _find(self, *sequences: str) -> int:
        if all(isinstance(s, str) for s in sequences):
            return _textdistance.ratcliff_obershelp_find(sequences)
        subseq = LCSStr()(*sequences)
        length = len(subseq)
        if length == 0:
            return 0
        before = [s[:s.find(subseq)] for s in sequences]
        after = [s[s.find(subseq) + length:] for s in sequences]
        return self._find(*before) + length + self._find(*after)

    def __call__(self, *sequences: str) -> float:
        result = self.quick_answer(*sequences)
        if result is not None:
            return result
        scount = len(sequences)  # sequences count
        ecount = sum(map(len, sequences))  # elements count
        sequences = self._get_sequences(*sequences)
        return scount * self._find(*sequences) / ecount


lcsseq = LCSSeq()
lcsstr = LCSStr()
ratcliff_obershelp = RatcliffObershelp()
