"""TextDistance: string distance and similarity algorithms.

Native Rust port of the MIT-licensed ``textdistance`` project. The algorithm
kernels are implemented in Rust and exposed through this adapter, which keeps
the original Python class API intact so existing code and the original test
suite run unmodified.
"""

# main package info
__title__ = 'TextDistance'
__version__ = '4.6.2'
VERSION = __version__


# app
from .algorithms import *  # noQA
from .utils import *  # noQA
