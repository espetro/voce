"""CPython-only stand-in for the FFI module when running spike.py as Python."""

import resource


def rss_kb() -> int:
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
