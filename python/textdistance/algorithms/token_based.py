from __future__ import annotations

# built-in
from functools import reduce
from itertools import islice, permutations, repeat
from math import log
from typing import Sequence

# app
from .base import Base as _Base, BaseSimilarity as _BaseSimilarity
from .edit_based import DamerauLevenshtein
from .. import _textdistance


__all__ = [
    'Jaccard', 'Sorensen', 'Tversky',
    'Overlap', 'Cosine', 'Tanimoto', 'MongeElkan', 'Bag',

    'jaccard', 'sorensen', 'tversky', 'sorensen_dice', 'dice',
    'overlap', 'cosine', 'tanimoto', 'monge_elkan', 'bag',
]


class Jaccard(_BaseSimilarity):
    """
    Compute the Jaccard similarity between the two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 1, where 1 means equal,
    and 0 totally different.

    https://en.wikipedia.org/wiki/Jaccard_index
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/jaccard.js
    """

    def __init__(
        self,
        qval: int = 1,
        as_set: bool = False,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.as_set = as_set
        self.external = external

    def maximum(self, *sequences: Sequence) -> int:
        return 1

    def __call__(self, *sequences: Sequence) -> float:
        result = self.quick_answer(*sequences)
        if result is not None:
            return result

        counters = self._get_counters(*sequences)               # sets
        intersection, union, _counts = _textdistance.token_stats(
            counters, getattr(self, 'as_set', False),
        )
        return intersection / union


class Sorensen(_BaseSimilarity):
    """
    Compute the Sorensen distance between the two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 1, where 1 means equal,
    and 0 totally different.

    https://en.wikipedia.org/wiki/S%C3%B8rensen%E2%80%93Dice_coefficient
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/dice.js
    """

    def __init__(self, qval: int = 1, as_set: bool = False, external: bool = True) -> None:
        self.qval = qval
        self.as_set = as_set
        self.external = external

    def maximum(self, *sequences: Sequence) -> int:
        return 1

    def __call__(self, *sequences: Sequence) -> float:
        result = self.quick_answer(*sequences)
        if result is not None:
            return result

        counters = self._get_counters(*sequences)               # sets
        intersection, union, counts = _textdistance.token_stats(
            counters, getattr(self, 'as_set', False),
        )
        count = sum(counts)
        return 2.0 * intersection / count


class Tversky(_BaseSimilarity):
    """
    Compute the Tversky index for two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 1, where 1 means equal,
    and 0 totally different

    https://en.wikipedia.org/wiki/Tversky_index
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/tversky.js
    """

    def __init__(
        self,
        qval: int = 1,
        ks: Sequence[float] = None,
        bias: float | None = None,
        as_set: bool = False,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.ks = ks or repeat(1)
        self.bias = bias
        self.as_set = as_set
        self.external = external

    def maximum(self, *sequences: Sequence) -> int:
        return 1

    def __call__(self, *sequences: Sequence) -> float:
        quick_result = self.quick_answer(*sequences)
        if quick_result is not None:
            return quick_result

        counters = self._get_counters(*sequences)                # sets
        intersection, union, counts = _textdistance.token_stats(
            counters, getattr(self, 'as_set', False),
        )
        ks = list(islice(self.ks, len(counts)))

        if len(counts) != 2 or self.bias is None:
            result = intersection
            for k, s in zip(ks, counts):
                result += k * (s - intersection)
            return intersection / result

        s1, s2 = counts
        alpha, beta = ks
        a_val = min([s1, s2])
        b_val = max([s1, s2])
        c_val = intersection + self.bias
        result = alpha * beta * (a_val - b_val) + b_val * beta
        return c_val / (result + c_val)


class Overlap(_BaseSimilarity):
    """
    Compute the Overlap coefficient for two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 1, where 1 means equal,
    and 0 totally different

    https://en.wikipedia.org/wiki/Overlap_coefficient
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/overlap.js
    """

    def __init__(
        self,
        qval: int = 1,
        as_set: bool = False,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.as_set = as_set
        self.external = external

    def maximum(self, *sequences: Sequence) -> int:
        return 1

    def __call__(self, *sequences: Sequence) -> float:
        result = self.quick_answer(*sequences)
        if result is not None:
            return result

        counters = self._get_counters(*sequences)                  # sets
        intersection, union, counts = _textdistance.token_stats(
            counters, getattr(self, 'as_set', False),
        )

        return intersection / min(counts)


class Cosine(_BaseSimilarity):
    """
    Compute the Cosine similarity (Ochiai coefficient) for two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 1, where 1 means equal,
    and 0 totally different

    https://en.wikipedia.org/wiki/Cosine_similarity
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/cosine.js
    """

    def __init__(
        self,
        qval: int = 1,
        as_set: bool = False,
        external: bool = True,
    ) -> None:
        self.qval = qval
        self.as_set = as_set
        self.external = external

    def maximum(self, *sequences: Sequence) -> int:
        return 1

    def __call__(self, *sequences: Sequence) -> float:
        result = self.quick_answer(*sequences)
        if result is not None:
            return result

        counters = self._get_counters(*sequences)                  # sets
        intersection, union, counts = _textdistance.token_stats(
            counters, getattr(self, 'as_set', False),
        )
        prod = reduce(lambda x, y: x * y, counts)

        return intersection / pow(prod, 1.0 / len(counts))


class Tanimoto(Jaccard):
    """
    Compute the Tanimoto distance between two sequences.
    They should contain hashable items.
    The return value is a float between -inf and 0, where 0 means equal,
    and -inf totally different

    This is identical to the Jaccard similarity coefficient
    and the Tversky index for alpha=1 and beta=1.

    https://en.wikipedia.org/wiki/Jaccard_index#Tanimoto_similarity_and_distance
    """

    def __call__(self, *sequences: Sequence) -> float:
        result = super().__call__(*sequences)
        if result == 0:
            return float('-inf')
        else:
            return log(result, 2)


class MongeElkan(_BaseSimilarity):
    """
    Compute the Monge Elkan distance between two sequences.
    They should contain hashable items.
    The return value is a float between 0 and 2, where 2 means equal,
    and 0 totally different.

    https://www.academia.edu/200314/Generalized_Monge-Elkan_Method_for_Approximate_Text_String_Comparison
    http://www.cs.cmu.edu/~wcohen/postscript/kdd-2003-match-ws.pdf
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/monge-elkan.js
    """
    _damerau_levenshtein = DamerauLevenshtein()

    def __init__(
        self,
        algorithm=_damerau_levenshtein,
        symmetric: bool = False,
        qval: int = 1,
        external: bool = True,
    ) -> None:
        self.algorithm = algorithm
        self.symmetric = symmetric
        self.qval = qval
        self.external = external

    def maximum(self, *sequences: Sequence) -> float:
        result = self.algorithm.maximum(sequences)
        for seq in sequences:
            if seq:
                result = max(result, self.algorithm.maximum(*seq))
        return result

    def _calc(self, seq, *sequences: Sequence) -> float:
        if not seq:
            return 0
        maxes = []
        for c1 in seq:
            for s in sequences:
                max_sim = float('-inf')
                for c2 in s:
                    max_sim = max(max_sim, self.algorithm.similarity(c1, c2))
                maxes.append(max_sim)
        return sum(maxes) / len(seq) / len(maxes)

    def __call__(self, *sequences: Sequence) -> float:
        quick_result = self.quick_answer(*sequences)
        if quick_result is not None:
            return quick_result
        sequences = self._get_sequences(*sequences)

        if self.symmetric:
            result = []
            for seqs in permutations(sequences):
                result.append(self._calc(*seqs))
            return sum(result) / len(result)
        else:
            return self._calc(*sequences)


class Bag(_Base):
    """
    Compute the Bag distance between two sequences.
    They should contain hashable items.
    The return value is a float between 0 and N, where 0 means equal,
    and N totally different. N would, at most, be the length of the
    longest sequence in the comparison.

    http://www-db.disi.unibo.it/research/papers/SPIRE02.pdf
    https://github.com/Yomguithereal/talisman/blob/master/src/metrics/bag.js
    """

    def __call__(self, *sequences: Sequence) -> float:
        counters = self._get_counters(*sequences)              # sets
        intersection = self._intersect_counters(*counters)     # set
        return max(self._count_counters(sequence - intersection) for sequence in counters)


bag = Bag()
cosine = Cosine()
dice = Sorensen()
jaccard = Jaccard()
monge_elkan = MongeElkan()
overlap = Overlap()
sorensen = Sorensen()
sorensen_dice = Sorensen()
# sorensen_dice = Tversky(ks=[.5, .5])
tanimoto = Tanimoto()
tversky = Tversky()
