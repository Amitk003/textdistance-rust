from __future__ import annotations

# built-in
from itertools import groupby, zip_longest
from typing import Any, Iterator, Sequence, TypeVar

# app
from .base import Base as _Base, BaseSimilarity as _BaseSimilarity
from .. import _textdistance


__all__ = [
    'MRA', 'Editex',
    'mra', 'editex',
]
T = TypeVar('T')


class MRA(_BaseSimilarity):
    """Western Airlines Surname Match Rating Algorithm comparison rating
    https://en.wikipedia.org/wiki/Match_rating_approach
    """

    def maximum(self, *sequences: str) -> int:
        sequences = [list(self._calc_mra(s)) for s in sequences]
        return max(map(len, sequences))

    def _calc_mra(self, word: str) -> str:
        if not word:
            return word
        word = word.upper()
        word = word[0] + ''.join(c for c in word[1:] if c not in 'AEIOU')
        # remove repeats like an UNIX uniq
        word = ''.join(char for char, _ in groupby(word))
        if len(word) > 6:
            return word[:3] + word[-3:]
        return word

    def __call__(self, *sequences: str) -> int:
        if not all(sequences):
            return 0
        sequences = [list(self._calc_mra(s)) for s in sequences]
        lengths = list(map(len, sequences))
        count = len(lengths)
        max_length = max(lengths)
        if abs(max_length - min(lengths)) > count:
            return 0

        for _ in range(count):
            new_sequences = []
            minlen = min(lengths)
            for chars in zip(*sequences):
                if not self._ident(*chars):
                    new_sequences.append(chars)
            new_sequences = map(list, zip(*new_sequences))
            # update sequences
            ss: Iterator[tuple[Any, Any]]
            ss = zip_longest(new_sequences, sequences, fillvalue=list())
            sequences = [s1 + s2[minlen:] for s1, s2 in ss]
            # update lengths
            lengths = list(map(len, sequences))

        if not lengths:
            return max_length
        return max_length - max(lengths)


class Editex(_Base):
    """Editex distance

    https://github.com/chrislit/blob/master/abydos/distance/_editex.py
    """
    groups: tuple[frozenset[str], ...] = (
        frozenset('AEIOUY'),
        frozenset('BP'),
        frozenset('CKQ'),
        frozenset('DT'),
        frozenset('LR'),
        frozenset('MN'),
        frozenset('GJ'),
        frozenset('FPV'),
        frozenset('SXZ'),
        frozenset('CSZ'),
    )
    ungrouped: frozenset[str] = frozenset('HW')

    def __init__(
        self,
        match_cost: int = 0,
        group_cost: int = 1,
        mismatch_cost: int = 2,
        local: bool = False,
        groups=None,
        ungrouped=None,
        external: bool = True,
    ) -> None:
        # Ensure that match_cost <= group_cost <= mismatch_cost
        self.match_cost = match_cost
        self.group_cost = max(group_cost, self.match_cost)
        self.mismatch_cost = max(mismatch_cost, self.group_cost)
        self.local = local
        self.external = external

        if groups is not None:
            if ungrouped is None:
                raise ValueError('`ungrouped` argument required with `groups`')
            self.groups = groups
            self.ungrouped = ungrouped
        self.grouped = frozenset.union(*self.groups)

    def maximum(self, *sequences: Sequence) -> int:
        return max(map(len, sequences)) * self.mismatch_cost

    def __call__(self, s1: str, s2: str) -> float:
        result = self.quick_answer(s1, s2)
        if result is not None:
            return result

        max_length = self.maximum(s1, s2)
        return _textdistance.editex(
            s1.upper(),
            s2.upper(),
            self.match_cost,
            self.group_cost,
            self.mismatch_cost,
            self.local,
            self.groups,
            self.ungrouped,
            max_length,
        )


mra = MRA()
editex = Editex()
