"""Plain string helpers with no framework imports.

Paired with geometry.py to give the zero-family fixture two ordinary source
files. Nothing here matches a supported framework anchor, so indexing must
produce zero families.
"""


def shout(text):
    return text.upper() + "!"


def repeat(text, times):
    return text * times


def initials(first, last):
    return first[0] + last[0]
