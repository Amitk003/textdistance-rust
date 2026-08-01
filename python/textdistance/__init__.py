"""TextDistance: string distance and similarity algorithms.

Native Rust port of the MIT-licensed ``textdistance`` project. The algorithm
kernels are implemented in Rust and exposed through this adapter, which keeps
the original Python class API intact so existing code and the original test
suite run unmodified.
"""

from . import _textdistance

__version__ = _textdistance.__version__
VERSION = __version__

__title__ = "TextDistance"

__all__ = []
