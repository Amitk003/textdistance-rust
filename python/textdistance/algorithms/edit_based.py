from __future__ import annotations

# built-in
from typing import Sequence, TypeVar

# upstream requires numpy for NeedlemanWunsch / SmithWaterman / Gotoh; mirror that dependency
try:
    import numpy  # noqa: F401
except ImportError:
    numpy = None

# app
from .base import Base as _Base, BaseSimilarity as _BaseSimilarity
from .types import SimFunc, TestFunc
from .. import _textdistance


__all__ = [
    'Hamming', 'MLIPNS',
    'Levenshtein', 'DamerauLevenshtein',
    'Jaro', 'JaroWinkler', 'StrCmp95',
    'NeedlemanWunsch', 'Gotoh', 'SmithWaterman',

    'hamming', 'mlipns',
    'levenshtein', 'damerau_levenshtein',
    'jaro', 'jaro_winkler', 'strcmp95',
    'needleman_wunsch', 'gotoh', 'smith_waterman',
]
T = TypeVar('T')


class Hamming(_Base):
    """
    Compute the Hamming distance between the two or more sequences.
    The Hamming distance is the number of differing items in ordered sequences.

    https://en.wikipedia.org/wiki/Hamming_distance
    """

    def __init__(
        self,
        qval: int = 1,
        test_func: TestFunc | None = None,
        truncate: bool = False,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.test_func = test_func or self._ident
        self.truncate = truncate
        self.external = external

    def __call__(self, *sequences: Sequence[object]) -> int:
        sequences = self._get_sequences(*sequences)

        result = self.quick_answer(*sequences)
        if result is not None:
            assert isinstance(result, int)
            return result

        test_func = None if self.test_func is self._ident else self.test_func
        return _textdistance.hamming(sequences, self.truncate, test_func)


class Levenshtein(_Base):
    """
    Compute the absolute Levenshtein distance between the two sequences.
    The Levenshtein distance is the minimum number of edit operations necessary
    for transforming one sequence into the other. The edit operations allowed are:

        * deletion:     ABC -> BC, AC, AB
        * insertion:    ABC -> ABCD, EABC, AEBC..
        * substitution: ABC -> ABE, ADC, FBC..

    https://en.wikipedia.org/wiki/Levenshtein_distance
    """

    def __init__(
        self,
        qval: int = 1,
        test_func: TestFunc | None = None,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.test_func = test_func or self._ident
        self.external = external

    def __call__(self, s1: Sequence[T], s2: Sequence[T]) -> int:
        s1, s2 = self._get_sequences(s1, s2)

        result = self.quick_answer(s1, s2)
        if result is not None:
            assert isinstance(result, int)
            return result

        test_func = None if self.test_func is self._ident else self.test_func
        return _textdistance.levenshtein(s1, s2, test_func)


class DamerauLevenshtein(_Base):
    """
    Compute the absolute Damerau-Levenshtein distance between the two sequences.
    The Damerau-Levenshtein distance is the minimum number of edit operations necessary
    for transforming one sequence into the other. The edit operations allowed are:

        * deletion:      ABC -> BC, AC, AB
        * insertion:     ABC -> ABCD, EABC, AEBC..
        * substitution:  ABC -> ABE, ADC, FBC..
        * transposition: ABC -> ACB, BAC

    If `restricted=False`, it will calculate unrestricted distance,
    where the same character can be touched more than once.
    So the distance between BA and ACB is 2: BA -> AB -> ACB.

    https://en.wikipedia.org/wiki/Damerau%E2%80%93Levenshtein_distance
    """

    def __init__(
        self,
        qval: int = 1,
        test_func: TestFunc | None = None,
        external: bool = True,
        restricted: bool = True,
    ) -> None:
        self.qval = qval
        self.test_func = test_func or self._ident
        self.external = external
        self.restricted = restricted

    def _pure_python_unrestricted(self, s1: Sequence[T], s2: Sequence[T]) -> int:
        test_func = None if self.test_func is self._ident else self.test_func
        return _textdistance.damerau_levenshtein(s1, s2, False, test_func)

    def _pure_python_restricted(self, s1: Sequence[T], s2: Sequence[T]) -> int:
        test_func = None if self.test_func is self._ident else self.test_func
        return _textdistance.damerau_levenshtein(s1, s2, True, test_func)

    def __call__(self, s1: Sequence[T], s2: Sequence[T]) -> int:
        s1, s2 = self._get_sequences(s1, s2)

        result = self.quick_answer(s1, s2)
        if result is not None:
            return result  # type: ignore[return-value]

        if self.restricted:
            return self._pure_python_restricted(s1, s2)
        return self._pure_python_unrestricted(s1, s2)


class JaroWinkler(_BaseSimilarity):
    """
    Computes the Jaro-Winkler measure between two strings.
    The Jaro-Winkler measure is designed to capture cases where two strings
    have a low Jaro score, but share a prefix.
    and thus are likely to match.

    https://en.wikipedia.org/wiki/Jaro%E2%80%93Winkler_distance
    """

    def __init__(
        self,
        long_tolerance: bool = False,
        winklerize: bool = True,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.long_tolerance = long_tolerance
        self.winklerize = winklerize
        self.external = external

    def maximum(self, *sequences: Sequence[object]) -> int:
        return 1

    def __call__(self, s1: Sequence[T], s2: Sequence[T], prefix_weight: float = 0.1) -> float:
        s1, s2 = self._get_sequences(s1, s2)

        result = self.quick_answer(s1, s2)
        if result is not None:
            return result

        return _textdistance.jaro_winkler(
            s1, s2, prefix_weight, self.long_tolerance, self.winklerize,
        )


class Jaro(JaroWinkler):
    def __init__(
        self,
        long_tolerance: bool = False,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        super().__init__(
            long_tolerance=long_tolerance,
            winklerize=False,
            qval=qval,
            external=external,
        )


class NeedlemanWunsch(_BaseSimilarity):
    """
    Computes the Needleman-Wunsch measure between two strings.
    The Needleman-Wunsch generalizes the Levenshtein distance and considers global
    alignment between two strings. Specifically, it is computed by assigning
    a score to each alignment between two input strings and choosing the
    score of the best alignment, that is, the maximal score.
    An alignment between two strings is a set of correspondences between the
    characters of between them, allowing for gaps.

    https://en.wikipedia.org/wiki/Needleman%E2%80%93Wunsch_algorithm
    """

    def __init__(
        self,
        gap_cost: float = 1.0,
        sim_func: SimFunc = None,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.gap_cost = gap_cost
        if sim_func:
            self.sim_func = sim_func
        else:
            self.sim_func = self._ident
        self.external = external

    def minimum(self, *sequences: Sequence[object]) -> float:
        return -max(map(len, sequences)) * self.gap_cost

    def maximum(self, *sequences: Sequence[object]) -> float:
        return max(map(len, sequences))

    def distance(self, *sequences: Sequence[object]) -> float:
        """Get distance between sequences
        """
        return -1 * self.similarity(*sequences)

    def normalized_distance(self, *sequences: Sequence[object]) -> float:
        """Get distance from 0 to 1
        """
        minimum = self.minimum(*sequences)
        maximum = self.maximum(*sequences)
        if maximum == 0:
            return 0
        return (self.distance(*sequences) - minimum) / (maximum - minimum)

    def normalized_similarity(self, *sequences: Sequence[object]) -> float:
        """Get similarity from 0 to 1
        """
        minimum = self.minimum(*sequences)
        maximum = self.maximum(*sequences)
        if maximum == 0:
            return 1
        return (self.similarity(*sequences) - minimum) / (maximum * 2)

    def __call__(self, s1: Sequence[T], s2: Sequence[T]) -> float:
        if not numpy:
            raise ImportError('Please, install numpy for Needleman-Wunsch measure')
        s1, s2 = self._get_sequences(s1, s2)
        sim_func = None if self.sim_func is self._ident else self.sim_func
        return _textdistance.needleman_wunsch(s1, s2, self.gap_cost, sim_func)


class SmithWaterman(_BaseSimilarity):
    """
    Computes the Smith-Waterman measure between two strings.
    The Smith-Waterman algorithm performs local sequence alignment;
    that is, for determining similar regions between two strings.
    Instead of looking at the total sequence, the Smith-Waterman algorithm compares
    segments of all possible lengths and optimizes the similarity measure.

    https://en.wikipedia.org/wiki/Smith%E2%80%93Waterman_algorithm
    """

    def __init__(
        self,
        gap_cost: float = 1.0,
        sim_func: SimFunc = None,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.gap_cost = gap_cost
        self.sim_func = sim_func or self._ident
        self.external = external

    def maximum(self, *sequences: Sequence[object]) -> int:
        return min(map(len, sequences))

    def __call__(self, s1: Sequence[T], s2: Sequence[T]) -> float:
        if not numpy:
            raise ImportError('Please, install numpy for Smith-Waterman measure')
        s1, s2 = self._get_sequences(s1, s2)

        result = self.quick_answer(s1, s2)
        if result is not None:
            return result

        sim_func = None if self.sim_func is self._ident else self.sim_func
        return _textdistance.smith_waterman(s1, s2, self.gap_cost, sim_func)


class Gotoh(NeedlemanWunsch):
    """Gotoh score
    Gotoh's algorithm is essentially Needleman-Wunsch with affine gap
    penalties:
    https://www.cs.umd.edu/class/spring2003/cmsc838t/papers/gotoh1982.pdf
    """

    def __init__(
        self,
        gap_open: int = 1,
        gap_ext: float = 0.4,
        sim_func: SimFunc = None,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.gap_open = gap_open
        self.gap_ext = gap_ext
        if sim_func:
            self.sim_func = sim_func
        else:
            self.sim_func = self._ident
        self.external = external

    def minimum(self, *sequences: Sequence[object]) -> int:
        return -min(map(len, sequences))

    def maximum(self, *sequences: Sequence[object]) -> int:
        return min(map(len, sequences))

    def __call__(self, s1: Sequence[T], s2: Sequence[T]) -> float:
        if not numpy:
            raise ImportError('Please, install numpy for Gotoh measure')
        s1, s2 = self._get_sequences(s1, s2)
        if (len(s1) == 0) != (len(s2) == 0):
            raise IndexError("index 1 is out of bounds for axis 0 with size 1")
        sim_func = None if self.sim_func is self._ident else self.sim_func
        return _textdistance.gotoh(s1, s2, self.gap_open, self.gap_ext, sim_func)


class StrCmp95(_BaseSimilarity):
    """strcmp95 similarity

    http://cpansearch.perl.org/src/SCW/Text-JaroWinkler-0.1/strcmp95.c
    """
    sp_mx: tuple[tuple[str, str], ...] = (
        ('A', 'E'), ('A', 'I'), ('A', 'O'), ('A', 'U'), ('B', 'V'), ('E', 'I'),
        ('E', 'O'), ('E', 'U'), ('I', 'O'), ('I', 'U'), ('O', 'U'), ('I', 'Y'),
        ('E', 'Y'), ('C', 'G'), ('E', 'F'), ('W', 'U'), ('W', 'V'), ('X', 'K'),
        ('S', 'Z'), ('X', 'S'), ('Q', 'C'), ('U', 'V'), ('M', 'N'), ('L', 'I'),
        ('Q', 'O'), ('P', 'R'), ('I', 'J'), ('2', 'Z'), ('5', 'S'), ('8', 'B'),
        ('1', 'I'), ('1', 'L'), ('0', 'O'), ('0', 'Q'), ('C', 'K'), ('G', 'J'),
    )

    def __init__(self, long_strings: bool = False, external: bool = True) -> None:
        self.long_strings = long_strings
        self.external = external

    def maximum(self, *sequences: Sequence[object]) -> int:
        return 1

    def __call__(self, s1: str, s2: str) -> float:
        s1 = s1.strip().upper()
        s2 = s2.strip().upper()

        result = self.quick_answer(s1, s2)
        if result is not None:
            return result

        return _textdistance.strcmp95(s1, s2, self.long_strings)


class MLIPNS(_BaseSimilarity):
    """
    Compute the Hamming distance between the two or more sequences.
    The Hamming distance is the number of differing items in ordered sequences.

    http://www.sial.iias.spb.su/files/386-386-1-PB.pdf
    """

    def __init__(
        self, threshold: float = 0.25,
        maxmismatches: int = 2,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.threshold = threshold
        self.maxmismatches = maxmismatches
        self.external = external

    def maximum(self, *sequences: Sequence[object]) -> int:
        return 1

    def __call__(self, *sequences: Sequence[object]) -> float:
        sequences = self._get_sequences(*sequences)

        result = self.quick_answer(*sequences)
        if result is not None:
            return result

        if len(sequences) == 2:
            # The original always returns 1 or 0, as ints (the N>2 loop below
            # does too). The kernel returns the same values as f64; narrow to
            # int so the return type matches the original exactly.
            return int(_textdistance.mlipns(
                sequences[0], sequences[1], self.threshold, self.maxmismatches,
            ))

        mismatches = 0
        ham = _textdistance.hamming(sequences, False, None)
        maxlen = max(map(len, sequences))
        while all(sequences) and mismatches <= self.maxmismatches:
            if not maxlen:
                return 1
            if 1 - (maxlen - ham) / maxlen <= self.threshold:
                return 1
            mismatches += 1
            ham -= 1
            maxlen -= 1

        if not maxlen:
            return 1
        return 0


hamming = Hamming()
levenshtein = Levenshtein()
damerau = damerau_levenshtein = DamerauLevenshtein()
jaro = Jaro()
jaro_winkler = JaroWinkler()
needleman_wunsch = NeedlemanWunsch()
smith_waterman = SmithWaterman()
gotoh = Gotoh()
strcmp95 = StrCmp95()
mlipns = MLIPNS()
