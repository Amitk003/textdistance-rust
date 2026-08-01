# Loaded for every pytest invocation in this repo.
#
# The upstream suite in tests/original is kept byte-for-byte; this file only
# tunes how hypothesis runs it. The compression tests draw very long unicode
# strings on which the arithmetic NCD is inherently slow: the original takes
# ~300 ms on such inputs, above the hypothesis default deadline of 200 ms, and
# upstream itself excludes lzma_ncd from CI as "too slow, makes CI flaky". The
# port therefore runs the suite with a more generous, still-finite deadline.
# See DECISIONS.md D19 for the evidence.

from hypothesis import settings

settings.register_profile("port", deadline=2000)
settings.load_profile("port")
